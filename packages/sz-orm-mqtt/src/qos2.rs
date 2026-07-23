//! # QoS 2 四次握手状态机
//!
//! 实现 MQTT QoS 2（Exactly Once）消息传输的完整四次握手流程：
//!
//! ```text
//! 发送方                接收方
//!   |  PUBLISH(packet_id)  ->    |
//!   |  <-  PUBREC(packet_id)     |
//!   |  PUBREL(packet_id)   ->    |
//!   |  <-  PUBCOMP(packet_id)    |
//! ```
//!
//! 发送方状态：Init -> Published -> Received -> Released -> Complete
//! 接收方状态：Init -> Received -> Released -> Complete
//!
//! 任何阶段异常可通过 [`PacketState`] 查询并支持重传。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// QoS 2 四次握手阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketState {
    /// 初始：尚未发送/接收 PUBLISH
    Init,
    /// 已发送/接收 PUBLISH
    Published,
    /// 已发送/接收 PUBREC（发送方收到，接收方已发送）
    Received,
    /// 已发送/接收 PUBREL
    Released,
    /// 已发送/接收 PUBCOMP，握手完成
    Complete,
}

impl PacketState {
    /// 是否处于握手进行中（未完成）
    pub fn in_progress(&self) -> bool {
        !matches!(self, PacketState::Complete)
    }

    /// 是否已完成
    pub fn is_complete(&self) -> bool {
        matches!(self, PacketState::Complete)
    }
}

/// 单个 QoS 2 数据包的握手跟踪记录
#[derive(Debug, Clone)]
pub struct Qos2Packet {
    /// 数据包 ID（MQTT 协议中 1-65535）
    pub packet_id: u16,
    /// 主题
    pub topic: String,
    /// 当前状态
    pub state: PacketState,
    /// 重试次数
    pub retries: u8,
}

impl Qos2Packet {
    pub fn new(packet_id: u16, topic: impl Into<String>) -> Self {
        Self {
            packet_id,
            topic: topic.into(),
            state: PacketState::Init,
            retries: 0,
        }
    }

    /// 推进到下一状态，返回是否成功
    pub fn advance(&mut self) -> bool {
        let next = match self.state {
            PacketState::Init => PacketState::Published,
            PacketState::Published => PacketState::Received,
            PacketState::Received => PacketState::Released,
            PacketState::Released => PacketState::Complete,
            PacketState::Complete => return false,
        };
        self.state = next;
        true
    }

    /// 重置回 Init（用于重传 PUBLISH）
    pub fn reset(&mut self) {
        self.state = PacketState::Init;
        self.retries = self.retries.saturating_add(1);
    }
}

/// 发送方视角的 QoS 2 状态跟踪表
#[derive(Debug, Default)]
pub struct SenderState {
    packets: Arc<RwLock<HashMap<u16, Qos2Packet>>>,
    /// 下一个未使用的 packet_id（自增）
    next_id: Arc<RwLock<u16>>,
}

impl SenderState {
    pub fn new() -> Self {
        Self {
            packets: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    /// 分配一个新的 packet_id（1-65535 循环使用）
    pub async fn allocate_id(&self) -> u16 {
        let mut next = self.next_id.write().await;
        let id = *next;
        *next = if *next == u16::MAX { 1 } else { *next + 1 };
        id
    }

    /// 注册一个新的 QoS 2 发送（PUBLISH 已发送）
    pub async fn register_publish(
        &self,
        packet_id: u16,
        topic: impl Into<String>,
    ) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        if packets.contains_key(&packet_id) {
            return Err(format!("packet_id {} already in use", packet_id));
        }
        let mut packet = Qos2Packet::new(packet_id, topic);
        packet.state = PacketState::Published;
        packets.insert(packet_id, packet);
        Ok(())
    }

    /// 标记收到 PUBREC（PUBLISH 已被对端确认）
    pub async fn receive_pubrec(&self, packet_id: u16) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        let packet = packets
            .get_mut(&packet_id)
            .ok_or_else(|| format!("unknown packet_id {}", packet_id))?;
        if packet.state != PacketState::Published {
            return Err(format!(
                "unexpected PUBREC for packet {} in state {:?}",
                packet_id, packet.state
            ));
        }
        packet.state = PacketState::Received;
        Ok(())
    }

    /// 标记已发送 PUBREL
    pub async fn send_pubrel(&self, packet_id: u16) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        let packet = packets
            .get_mut(&packet_id)
            .ok_or_else(|| format!("unknown packet_id {}", packet_id))?;
        if packet.state != PacketState::Received {
            return Err(format!(
                "unexpected PUBREL send for packet {} in state {:?}",
                packet_id, packet.state
            ));
        }
        packet.state = PacketState::Released;
        Ok(())
    }

    /// 标记收到 PUBCOMP，握手完成
    pub async fn receive_pubcomp(&self, packet_id: u16) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        let packet = packets
            .get_mut(&packet_id)
            .ok_or_else(|| format!("unknown packet_id {}", packet_id))?;
        if packet.state != PacketState::Released {
            return Err(format!(
                "unexpected PUBCOMP for packet {} in state {:?}",
                packet_id, packet.state
            ));
        }
        packet.state = PacketState::Complete;
        // 完成后从跟踪表中移除
        packets.remove(&packet_id);
        Ok(())
    }

    /// 查询某 packet 的当前状态
    pub async fn state_of(&self, packet_id: u16) -> Option<PacketState> {
        let packets = self.packets.read().await;
        packets.get(&packet_id).map(|p| p.state)
    }

    /// 当前进行中（未完成）的握手数量
    pub async fn in_progress_count(&self) -> usize {
        let packets = self.packets.read().await;
        packets.values().filter(|p| p.state.in_progress()).count()
    }

    /// 当前跟踪的所有 packet_id 列表
    pub async fn tracked_ids(&self) -> Vec<u16> {
        let packets = self.packets.read().await;
        let mut ids: Vec<u16> = packets.keys().copied().collect();
        ids.sort();
        ids
    }
}

/// 接收方视角的 QoS 2 状态跟踪表
#[derive(Debug, Default)]
pub struct ReceiverState {
    packets: Arc<RwLock<HashMap<u16, Qos2Packet>>>,
}

impl ReceiverState {
    pub fn new() -> Self {
        Self {
            packets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 收到 PUBLISH：注册并返回 PUBREC 应发送
    pub async fn receive_publish(
        &self,
        packet_id: u16,
        topic: impl Into<String>,
    ) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        if packets.contains_key(&packet_id) {
            // 重复 PUBLISH：可能是对端重传，保持原状态返回
            return Err(format!("duplicate PUBLISH for packet_id {}", packet_id));
        }
        let mut packet = Qos2Packet::new(packet_id, topic);
        packet.state = PacketState::Published;
        packets.insert(packet_id, packet);
        Ok(())
    }

    /// 标记已发送 PUBREC
    pub async fn send_pubrec(&self, packet_id: u16) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        let packet = packets
            .get_mut(&packet_id)
            .ok_or_else(|| format!("unknown packet_id {}", packet_id))?;
        if packet.state != PacketState::Published {
            return Err(format!(
                "unexpected PUBREC send for packet {} in state {:?}",
                packet_id, packet.state
            ));
        }
        packet.state = PacketState::Received;
        Ok(())
    }

    /// 收到 PUBREL：标记并返回 PUBCOMP 应发送
    pub async fn receive_pubrel(&self, packet_id: u16) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        let packet = packets
            .get_mut(&packet_id)
            .ok_or_else(|| format!("unknown packet_id {}", packet_id))?;
        if packet.state != PacketState::Received {
            return Err(format!(
                "unexpected PUBREL for packet {} in state {:?}",
                packet_id, packet.state
            ));
        }
        packet.state = PacketState::Released;
        Ok(())
    }

    /// 标记已发送 PUBCOMP，握手完成并清除
    pub async fn send_pubcomp(&self, packet_id: u16) -> Result<(), String> {
        let mut packets = self.packets.write().await;
        let packet = packets
            .get_mut(&packet_id)
            .ok_or_else(|| format!("unknown packet_id {}", packet_id))?;
        if packet.state != PacketState::Released {
            return Err(format!(
                "unexpected PUBCOMP send for packet {} in state {:?}",
                packet_id, packet.state
            ));
        }
        packet.state = PacketState::Complete;
        packets.remove(&packet_id);
        Ok(())
    }

    /// 查询状态
    pub async fn state_of(&self, packet_id: u16) -> Option<PacketState> {
        let packets = self.packets.read().await;
        packets.get(&packet_id).map(|p| p.state)
    }

    /// 当前进行中的握手数量
    pub async fn in_progress_count(&self) -> usize {
        let packets = self.packets.read().await;
        packets.values().filter(|p| p.state.in_progress()).count()
    }
}

/// 完整的 QoS 2 四次握手模拟器，用于测试与教学
pub struct Qos2HandshakeSimulator {
    sender: SenderState,
    receiver: ReceiverState,
}

impl Qos2HandshakeSimulator {
    pub fn new() -> Self {
        Self {
            sender: SenderState::new(),
            receiver: ReceiverState::new(),
        }
    }

    /// 模拟一次完整的 QoS 2 消息发送流程
    ///
    /// 返回 Ok(()) 表示握手完成，Err 描述失败原因
    pub async fn simulate(&self, packet_id: u16, topic: &str) -> Result<(), String> {
        // 1. 发送方 PUBLISH
        self.sender.register_publish(packet_id, topic).await?;
        // 2. 接收方接收 PUBLISH
        self.receiver.receive_publish(packet_id, topic).await?;
        // 3. 接收方发送 PUBREC
        self.receiver.send_pubrec(packet_id).await?;
        // 4. 发送方接收 PUBREC
        self.sender.receive_pubrec(packet_id).await?;
        // 5. 发送方发送 PUBREL
        self.sender.send_pubrel(packet_id).await?;
        // 6. 接收方接收 PUBREL
        self.receiver.receive_pubrel(packet_id).await?;
        // 7. 接收方发送 PUBCOMP
        self.receiver.send_pubcomp(packet_id).await?;
        // 8. 发送方接收 PUBCOMP
        self.sender.receive_pubcomp(packet_id).await?;
        Ok(())
    }

    pub fn sender(&self) -> &SenderState {
        &self.sender
    }

    pub fn receiver(&self) -> &ReceiverState {
        &self.receiver
    }
}

impl Default for Qos2HandshakeSimulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_state_in_progress() {
        assert!(PacketState::Init.in_progress());
        assert!(PacketState::Published.in_progress());
        assert!(PacketState::Received.in_progress());
        assert!(PacketState::Released.in_progress());
        assert!(!PacketState::Complete.in_progress());
    }

    #[test]
    fn test_packet_state_is_complete() {
        assert!(!PacketState::Init.is_complete());
        assert!(PacketState::Complete.is_complete());
    }

    #[test]
    fn test_qos2_packet_new() {
        let p = Qos2Packet::new(42, "test/topic");
        assert_eq!(p.packet_id, 42);
        assert_eq!(p.topic, "test/topic");
        assert_eq!(p.state, PacketState::Init);
        assert_eq!(p.retries, 0);
    }

    #[test]
    fn test_qos2_packet_advance_full_cycle() {
        let mut p = Qos2Packet::new(1, "t");
        assert!(p.advance()); // Init -> Published
        assert_eq!(p.state, PacketState::Published);
        assert!(p.advance()); // Published -> Received
        assert_eq!(p.state, PacketState::Received);
        assert!(p.advance()); // Received -> Released
        assert_eq!(p.state, PacketState::Released);
        assert!(p.advance()); // Released -> Complete
        assert_eq!(p.state, PacketState::Complete);
        assert!(!p.advance()); // Complete -> 无变化
    }

    #[test]
    fn test_qos2_packet_reset_increments_retries() {
        let mut p = Qos2Packet::new(1, "t");
        p.advance();
        assert_eq!(p.retries, 0);
        p.reset();
        assert_eq!(p.state, PacketState::Init);
        assert_eq!(p.retries, 1);
    }

    #[tokio::test]
    async fn test_sender_allocate_id_increments() {
        let s = SenderState::new();
        let id1 = s.allocate_id().await;
        let id2 = s.allocate_id().await;
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[tokio::test]
    async fn test_sender_allocate_id_wraps_at_max() {
        let s = SenderState::new();
        // 预设 next_id 为 u16::MAX
        {
            let mut next = s.next_id.write().await;
            *next = u16::MAX;
        }
        let id = s.allocate_id().await;
        assert_eq!(id, u16::MAX);
        let next_id = s.allocate_id().await;
        assert_eq!(next_id, 1);
    }

    #[tokio::test]
    async fn test_sender_register_publish() {
        let s = SenderState::new();
        s.register_publish(100, "topic/a").await.unwrap();
        assert_eq!(s.state_of(100).await, Some(PacketState::Published));
        assert_eq!(s.in_progress_count().await, 1);
    }

    #[tokio::test]
    async fn test_sender_register_duplicate_packet_id_fails() {
        let s = SenderState::new();
        s.register_publish(1, "t").await.unwrap();
        let result = s.register_publish(1, "t").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sender_full_handshake() {
        let s = SenderState::new();
        s.register_publish(50, "test").await.unwrap();
        assert_eq!(s.state_of(50).await, Some(PacketState::Published));

        s.receive_pubrec(50).await.unwrap();
        assert_eq!(s.state_of(50).await, Some(PacketState::Received));

        s.send_pubrel(50).await.unwrap();
        assert_eq!(s.state_of(50).await, Some(PacketState::Released));

        s.receive_pubcomp(50).await.unwrap();
        // 完成后从表中移除
        assert_eq!(s.state_of(50).await, None);
        assert_eq!(s.in_progress_count().await, 0);
    }

    #[tokio::test]
    async fn test_sender_pubrec_for_unknown_packet_fails() {
        let s = SenderState::new();
        let result = s.receive_pubrec(999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sender_pubrec_in_wrong_state_fails() {
        let s = SenderState::new();
        s.register_publish(1, "t").await.unwrap();
        s.receive_pubrec(1).await.unwrap();
        // 已收到 PUBREC，再次 receive_pubrec 应失败
        let result = s.receive_pubrec(1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sender_pubrel_in_wrong_state_fails() {
        let s = SenderState::new();
        s.register_publish(1, "t").await.unwrap();
        // 未收到 PUBREC 就发送 PUBREL
        let result = s.send_pubrel(1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sender_pubcomp_in_wrong_state_fails() {
        let s = SenderState::new();
        s.register_publish(1, "t").await.unwrap();
        s.receive_pubrec(1).await.unwrap();
        // 未发送 PUBREL 就收到 PUBCOMP
        let result = s.receive_pubcomp(1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sender_tracked_ids_sorted() {
        let s = SenderState::new();
        s.register_publish(3, "t").await.unwrap();
        s.register_publish(1, "t").await.unwrap();
        s.register_publish(2, "t").await.unwrap();
        assert_eq!(s.tracked_ids().await, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_receiver_full_handshake() {
        let r = ReceiverState::new();
        r.receive_publish(7, "t").await.unwrap();
        assert_eq!(r.state_of(7).await, Some(PacketState::Published));

        r.send_pubrec(7).await.unwrap();
        assert_eq!(r.state_of(7).await, Some(PacketState::Received));

        r.receive_pubrel(7).await.unwrap();
        assert_eq!(r.state_of(7).await, Some(PacketState::Released));

        r.send_pubcomp(7).await.unwrap();
        assert_eq!(r.state_of(7).await, None);
    }

    #[tokio::test]
    async fn test_receiver_duplicate_publish_fails() {
        let r = ReceiverState::new();
        r.receive_publish(1, "t").await.unwrap();
        let result = r.receive_publish(1, "t").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_receiver_pubrel_without_pubrec_fails() {
        let r = ReceiverState::new();
        r.receive_publish(1, "t").await.unwrap();
        // 未发送 PUBREC 就接收 PUBREL
        let result = r.receive_pubrel(1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_simulator_full_handshake_success() {
        let sim = Qos2HandshakeSimulator::new();
        let result = sim.simulate(123, "test/topic").await;
        assert!(result.is_ok());
        assert_eq!(sim.sender().in_progress_count().await, 0);
        assert_eq!(sim.receiver().in_progress_count().await, 0);
    }

    #[tokio::test]
    async fn test_simulator_tracks_state_during_handshake() {
        let sim = Qos2HandshakeSimulator::new();
        // 手动执行部分握手以验证中间状态
        sim.sender.register_publish(1, "t").await.unwrap();
        sim.receiver.receive_publish(1, "t").await.unwrap();
        sim.receiver.send_pubrec(1).await.unwrap();
        sim.sender.receive_pubrec(1).await.unwrap();

        assert_eq!(
            sim.sender().state_of(1).await,
            Some(PacketState::Received)
        );
        assert_eq!(
            sim.receiver().state_of(1).await,
            Some(PacketState::Received)
        );
    }

    #[tokio::test]
    async fn test_simulator_multiple_concurrent_handshakes() {
        let sim = Qos2HandshakeSimulator::new();
        sim.simulate(1, "topic/a").await.unwrap();
        sim.simulate(2, "topic/b").await.unwrap();
        sim.simulate(3, "topic/c").await.unwrap();
        assert_eq!(sim.sender().in_progress_count().await, 0);
    }
}
