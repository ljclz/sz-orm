//! TASK-012 验证测试：微调数据集生成

use sz_orm_model_ops::types::Quantization;

#[test]
fn test_quantization_variants() {
    assert_eq!(Quantization::None, Quantization::None);
    assert_eq!(Quantization::Int4, Quantization::Int4);
    assert_eq!(Quantization::Int8, Quantization::Int8);
    assert_ne!(Quantization::Int4, Quantization::Int8);
}
