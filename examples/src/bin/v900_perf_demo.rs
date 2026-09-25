//! v9.0.0 性能优化示例
//!
//! 演示 known_good 快速路径、clonecheap、reap_idle_selective 三项优化。

use sz_orm_core::Value;

#[tokio::main]
async fn main() {
    println!("=== sz-orm v9.0.0 性能优化示例 ===\n");

    // 1. known_good 快速路径
    println!("1. known_good 快速路径");
    println!("   连接释放后 known_good=true，再次 acquire 时跳过 is_connected() 检查");
    println!("   透明优化，无需修改业务代码\n");

    // 2. clonecheap
    println!("2. clonecheap 零堆分配克隆");
    let val = Value::I64(42);
    let cloned = val.clonecheap();
    println!("   原值: {:?}, 克隆: {:?}（Copy 变体零堆分配）\n", val, cloned);

    let str_val = Value::String("hello".into());
    let str_cloned = str_val.clonecheap();
    println!("   字符串值: {:?}, 克隆: {:?}（非 Copy 变体退化为 clone）\n", str_val, str_cloned);

    // 3. reap_idle_selective
    println!("3. reap_idle_selective 选择性回收");
    println!("   逐个 pop 检查，遇到未过期连接即停止");
    println!("   减少不必要的 pop/push 循环\n");

    println!("=== 示例完成 ===");
}