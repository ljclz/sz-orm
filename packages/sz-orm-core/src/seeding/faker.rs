//! FakerGenerator — faker 数据生成器
//!
//! 按字段类型生成随机/语义化假数据，支持字段语义自定义生成器。

use super::{FieldDef, FieldGenerator, FieldType, ModelDef, Record};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Faker 数据生成器
pub struct FakerGenerator {
    field_generators: HashMap<String, Box<dyn FieldGenerator>>,
    rng: StdRng,
}

impl FakerGenerator {
    /// 创建新的 FakerGenerator，注册内置字段生成器
    pub fn new() -> Self {
        let mut gen = Self {
            field_generators: HashMap::new(),
            rng: StdRng::from_entropy(),
        };
        gen.register_defaults();
        gen
    }

    /// 创建带种子 rng 的 FakerGenerator（可复现）
    pub fn with_seed(seed: u64) -> Self {
        let mut gen = Self {
            field_generators: HashMap::new(),
            rng: StdRng::seed_from_u64(seed),
        };
        gen.register_defaults();
        gen
    }

    fn register_defaults(&mut self) {
        self.register("name", Box::new(NameGenerator));
        self.register("email", Box::new(EmailGenerator));
        self.register("address", Box::new(AddressGenerator));
        self.register("phone", Box::new(PhoneGenerator));
        self.register("uuid", Box::new(UuidGenerator));
        self.register("date", Box::new(DateGenerator));
        self.register("boolean", Box::new(BooleanGenerator));
    }

    /// 注册字段语义自定义生成器
    pub fn register(&mut self, semantic: &str, generator: Box<dyn FieldGenerator>) {
        self.field_generators
            .insert(semantic.to_string(), generator);
    }

    /// 按字段类型推断默认生成器
    pub fn infer_generator(field_type: &FieldType) -> Box<dyn FieldGenerator> {
        match field_type {
            FieldType::String => Box::new(NameGenerator),
            FieldType::I32 | FieldType::U32 => Box::new(NumberGenerator::new(1, 100)),
            FieldType::I64 | FieldType::U64 => Box::new(NumberGenerator::new(1, 10000)),
            FieldType::F64 => Box::new(FloatGenerator::new(0.0, 100.0)),
            FieldType::Boolean => Box::new(BooleanGenerator),
            FieldType::Uuid => Box::new(UuidGenerator),
            FieldType::DateTime => Box::new(DateGenerator),
            FieldType::Json => Box::new(JsonGenerator),
            FieldType::Enum(variants) => Box::new(EnumGenerator::new(variants.clone())),
        }
    }

    /// 批量生成记录
    pub fn generate_batch(&mut self, model: &ModelDef, count: usize) -> Vec<Record> {
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let mut record = serde_json::Map::new();
            for field in &model.fields {
                let value = self.generate_field(field);
                record.insert(field.name.clone(), value);
            }
            records.push(record);
        }
        records
    }

    fn generate_field(&mut self, field: &FieldDef) -> Value {
        if field.nullable && self.rng.gen_bool(0.1) {
            return Value::Null;
        }
        let semantic_key = format!("{}.{}", "", field.name);
        if let Some(gen) = self.field_generators.get(&field.name) {
            return gen.generate(&mut self.rng);
        }
        if let Some(gen) = self.field_generators.get(&semantic_key) {
            return gen.generate(&mut self.rng);
        }
        let gen = Self::infer_generator(&field.field_type);
        gen.generate(&mut self.rng)
    }
}

impl Default for FakerGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ===== 内置字段生成器 =====

/// 姓名生成器（中文常见姓名）
pub struct NameGenerator;
impl FieldGenerator for NameGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        const NAMES: &[&str] = &[
            "张伟", "王芳", "李娜", "刘洋", "陈静", "杨磊", "赵敏", "黄强", "周丽", "吴杰", "徐涛",
            "孙艳", "马超", "朱琳", "胡军", "郭雪", "何明", "高飞", "林秀", "罗刚",
        ];
        Value::String(NAMES[rng.gen_range(0..NAMES.len())].to_string())
    }
}

/// 邮箱生成器
pub struct EmailGenerator;
impl FieldGenerator for EmailGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        const DOMAINS: &[&str] = &[
            "gmail.com",
            "qq.com",
            "163.com",
            "outlook.com",
            "huawei.com",
        ];
        let id: u64 = rng.gen_range(1000..99999);
        let domain = DOMAINS[rng.gen_range(0..DOMAINS.len())];
        Value::String(format!("user{}@{}", id, domain))
    }
}

/// 地址生成器
pub struct AddressGenerator;
impl FieldGenerator for AddressGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        const CITIES: &[&str] = &[
            "北京", "上海", "广州", "深圳", "杭州", "成都", "武汉", "西安",
        ];
        let city = CITIES[rng.gen_range(0..CITIES.len())];
        let street: u32 = rng.gen_range(1..999);
        Value::String(format!("{}市XX路{}号", city, street))
    }
}

/// 手机号生成器
pub struct PhoneGenerator;
impl FieldGenerator for PhoneGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        let prefix: u32 = rng.gen_range(130..190);
        let suffix: u64 = rng.gen_range(10000000..99999999);
        Value::String(format!("{}{:08}", prefix, suffix))
    }
}

/// UUID 生成器
pub struct UuidGenerator;
impl FieldGenerator for UuidGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        let bytes: [u8; 16] = rng.gen();
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        )
        .into()
    }
}

/// 日期生成器（YYYY-MM-DD 格式）
pub struct DateGenerator;
impl FieldGenerator for DateGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        let year: u32 = rng.gen_range(1970..2100);
        let month: u32 = rng.gen_range(1..13);
        let day: u32 = rng.gen_range(1..29);
        Value::String(format!("{:04}-{:02}-{:02}", year, month, day))
    }
}

/// 整数生成器（指定范围）
pub struct NumberGenerator {
    min: i64,
    max: i64,
}
impl NumberGenerator {
    /// 创建指定范围的整数生成器
    pub fn new(min: i64, max: i64) -> Self {
        Self { min, max }
    }
}
impl FieldGenerator for NumberGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        json!(rng.gen_range(self.min..self.max))
    }
}

/// 浮点数生成器（指定范围）
pub struct FloatGenerator {
    min: f64,
    max: f64,
}
impl FloatGenerator {
    /// 创建指定范围的浮点数生成器
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }
}
impl FieldGenerator for FloatGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        let val = rng.gen_range(self.min..self.max);
        json!(val)
    }
}

/// 布尔生成器
pub struct BooleanGenerator;
impl FieldGenerator for BooleanGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        Value::Bool(rng.gen_bool(0.5))
    }
}

/// 枚举生成器（从给定变体中随机选择）
pub struct EnumGenerator {
    variants: Vec<String>,
}
impl EnumGenerator {
    /// 创建枚举生成器
    pub fn new(variants: Vec<String>) -> Self {
        Self { variants }
    }
}
impl FieldGenerator for EnumGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        if self.variants.is_empty() {
            return Value::Null;
        }
        Value::String(self.variants[rng.gen_range(0..self.variants.len())].clone())
    }
}

/// JSON 生成器
pub struct JsonGenerator;
impl FieldGenerator for JsonGenerator {
    fn generate(&self, rng: &mut StdRng) -> Value {
        json!({
            "id": rng.gen::<u32>(),
            "active": rng.gen_bool(0.5),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeding::FieldDef;

    fn user_model() -> ModelDef {
        ModelDef {
            table: "users".to_string(),
            fields: vec![
                FieldDef {
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldDef {
                    name: "email".to_string(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldDef {
                    name: "age".to_string(),
                    field_type: FieldType::U32,
                    nullable: false,
                },
            ],
        }
    }

    #[test]
    fn test_generate_batch_basic() {
        let mut faker = FakerGenerator::with_seed(42);
        let model = user_model();
        let records = faker.generate_batch(&model, 10);
        assert_eq!(records.len(), 10);
        for record in &records {
            assert!(record.contains_key("name"));
            assert!(record.contains_key("email"));
            assert!(record.contains_key("age"));
            let name = record["name"].as_str().unwrap();
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_email_generator_format() {
        let mut faker = FakerGenerator::with_seed(42);
        let model = ModelDef {
            table: "test".to_string(),
            fields: vec![FieldDef {
                name: "email".to_string(),
                field_type: FieldType::String,
                nullable: false,
            }],
        };
        let records = faker.generate_batch(&model, 5);
        for record in &records {
            let email = record["email"].as_str().unwrap();
            assert!(email.contains('@'));
        }
    }

    #[test]
    fn test_register_custom_generator() {
        let mut faker = FakerGenerator::with_seed(42);
        faker.register("custom_field", Box::new(NameGenerator));
        let model = ModelDef {
            table: "test".to_string(),
            fields: vec![FieldDef {
                name: "custom_field".to_string(),
                field_type: FieldType::String,
                nullable: false,
            }],
        };
        let records = faker.generate_batch(&model, 3);
        assert_eq!(records.len(), 3);
        for record in &records {
            assert!(record["custom_field"].is_string());
        }
    }

    #[test]
    fn test_nullable_field() {
        let mut faker = FakerGenerator::with_seed(42);
        let model = ModelDef {
            table: "test".to_string(),
            fields: vec![FieldDef {
                name: "opt".to_string(),
                field_type: FieldType::String,
                nullable: true,
            }],
        };
        let records = faker.generate_batch(&model, 100);
        let null_count = records.iter().filter(|r| r["opt"].is_null()).count();
        assert!(null_count > 0, "nullable field should produce some nulls");
    }

    #[test]
    fn test_uuid_generator_format() {
        let mut rng = StdRng::seed_from_u64(42);
        let val = UuidGenerator.generate(&mut rng);
        let uuid_str = val.as_str().unwrap();
        assert_eq!(uuid_str.len(), 36);
        assert_eq!(uuid_str.chars().nth(8), Some('-'));
        assert_eq!(uuid_str.chars().nth(13), Some('-'));
        assert_eq!(uuid_str.chars().nth(18), Some('-'));
        assert_eq!(uuid_str.chars().nth(23), Some('-'));
    }

    #[test]
    fn test_enum_generator() {
        let mut rng = StdRng::seed_from_u64(42);
        let gen = EnumGenerator::new(vec!["active".into(), "inactive".into(), "banned".into()]);
        let val = gen.generate(&mut rng);
        let s = val.as_str().unwrap();
        assert!(["active", "inactive", "banned"].contains(&s));
    }

    #[test]
    fn test_infer_generator() {
        let gen = FakerGenerator::infer_generator(&FieldType::Boolean);
        let mut rng = StdRng::seed_from_u64(42);
        let val = gen.generate(&mut rng);
        assert!(val.is_boolean());
    }
}
