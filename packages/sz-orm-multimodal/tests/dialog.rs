//! TASK-030 集成测试：多模态多轮对话端到端验证

use sz_orm_multimodal::dialog::{ContextWindow, DialogMessage, MessageRole, MultimodalDialog};
use sz_orm_multimodal::types::Modality;

#[test]
fn test_dialog_text_flow() {
    let mut dialog = MultimodalDialog::new(10000);

    let user_msg = dialog.input("查询所有用户", Modality::Text).unwrap();
    assert_eq!(user_msg.role, MessageRole::User);
    assert_eq!(user_msg.modality, Modality::Text);

    let assistant_msg = dialog.reply("SELECT * FROM users");
    assert_eq!(assistant_msg.role, MessageRole::Assistant);

    assert_eq!(dialog.turn_count(), 1);
    assert_eq!(dialog.context().messages.len(), 2);
}

#[test]
fn test_dialog_voice_modality() {
    let mut dialog = MultimodalDialog::new(10000);

    dialog.input("语音查询", Modality::Voice).unwrap();
    assert_eq!(*dialog.current_modality(), Modality::Voice);

    let reply = dialog.reply("正在处理");
    assert_eq!(reply.modality, Modality::Voice);
}

#[test]
fn test_dialog_er_diagram_modality() {
    let mut dialog = MultimodalDialog::new(10000);

    dialog
        .input("画一个用户订单 ER 图", Modality::ErDiagram)
        .unwrap();
    assert_eq!(*dialog.current_modality(), Modality::ErDiagram);

    let reply = dialog.reply("已生成 ER 图");
    assert_eq!(reply.modality, Modality::ErDiagram);
}

#[test]
fn test_context_window_eviction() {
    let mut dialog = MultimodalDialog::new(5);

    for i in 0..10 {
        dialog.input(&format!("msg {}", i), Modality::Text).unwrap();
        dialog.reply("ok");
    }

    assert!(
        dialog.context().current_tokens <= 5,
        "Token 数不应超过上限: {}",
        dialog.context().current_tokens
    );
}

#[test]
fn test_switch_modality_mid_dialog() {
    let mut dialog = MultimodalDialog::new(10000);

    dialog.input("文本查询", Modality::Text).unwrap();
    dialog.reply("SELECT * FROM users");

    dialog.switch_modality(Modality::Voice);
    dialog.input("语音追问", Modality::Voice).unwrap();
    dialog.reply("已处理语音追问");

    assert_eq!(*dialog.current_modality(), Modality::Voice);
    assert_eq!(dialog.turn_count(), 2);
}

#[test]
fn test_clear_dialog() {
    let mut dialog = MultimodalDialog::new(10000);

    dialog.input("测试", Modality::Text).unwrap();
    dialog.reply("回复");
    assert_eq!(dialog.turn_count(), 1);

    dialog.clear();
    assert_eq!(dialog.turn_count(), 0);
    assert_eq!(dialog.context().messages.len(), 0);
}

#[test]
fn test_empty_input_fails() {
    let mut dialog = MultimodalDialog::new(10000);
    assert!(dialog.input("", Modality::Text).is_err());
}

#[test]
fn test_context_window_serialization() {
    let mut ctx = ContextWindow::new(1000);
    ctx.add_message(DialogMessage {
        role: MessageRole::User,
        content: "test".to_string(),
        modality: Modality::Text,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    });

    let json = serde_json::to_string(&ctx).unwrap();
    let restored: ContextWindow = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.messages.len(), 1);
}

#[test]
fn test_multi_turn_with_mixed_modalities() {
    let mut dialog = MultimodalDialog::new(10000);

    dialog.input("文本查询用户", Modality::Text).unwrap();
    dialog.reply("SELECT * FROM users");

    dialog.input("语音追问订单", Modality::Voice).unwrap();
    dialog.reply("SELECT * FROM orders");

    dialog.input("画 ER 图", Modality::ErDiagram).unwrap();
    dialog.reply("ER 图已生成");

    assert_eq!(dialog.turn_count(), 3);
    assert_eq!(dialog.context().messages.len(), 6);
}
