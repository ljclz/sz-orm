//! Fuzz 测试套件
//!
//! 针对输入边界、恶意输入、SQL 注入进行随机测试
//! 使用自定义伪随机数生成器，不依赖外部 fuzz 库

mod common;

use common::Rng;
use sz_orm_core::dialect::{ColumnDef, TableChange};
use sz_orm_core::DbType;
use sz_orm_core::QueryBuilder;
use sz_orm_core::Value;
use sz_orm_core::{get_dialect, Dialect, MySqlDialect, PostgreSqlDialect, SqliteDialect};
use sz_orm_core::{Model, ModelExt};

/// 测试用的简单 Model
struct FuzzModel;
impl Model for FuzzModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "fuzz_table"
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

const FUZZ_ITERATIONS: usize = 1000;

/// Fuzz Value::to_param：验证生成的 SQL 字面量是"安全的"（不包含未转义的单引号）
/// 不变量：to_param 输出的字符串中，单引号必须成对出现（字符串字面量边界除外）
#[test]
fn fuzz_value_to_param_safety() {
    let mut rng = Rng::new(42);
    for _ in 0..FUZZ_ITERATIONS {
        let v = generate_random_value(&mut rng);
        let param = v.to_param().to_string();

        // 验证：对于字符串类 Value，单引号必须成对出现
        match &v {
            Value::String(s)
            | Value::Json(s)
            | Value::Uuid(s)
            | Value::Date(s)
            | Value::DateTime(s)
            | Value::Time(s) => {
                // 验证原字符串中的单引号已被转义
                // to_param 输出格式：'escaped_content'
                // escaped_content 中的每个单引号应该变成两个单引号（或反斜杠转义）
                assert!(
                    param.starts_with('\'') && param.ends_with('\''),
                    "String param should be wrapped in quotes: {} (original: {:?})",
                    param,
                    s
                );
                // 提取内容部分（去掉外层引号）
                let content = &param[1..param.len() - 1];
                // 验证内容中不包含未转义的单引号
                // v0.2.1 修复 D-1：escape_string 只用 '' 转义，不再使用反斜杠转义
                let mut i = 0;
                let chars: Vec<char> = content.chars().collect();
                while i < chars.len() {
                    if chars[i] == '\'' {
                        // 必须是 '' 转义（下一个字符也是 '）
                        if i + 1 < chars.len() && chars[i + 1] == '\'' {
                            i += 2;
                            continue;
                        }
                        panic!(
                            "Unescaped single quote in param: {} (original: {:?})",
                            param, s
                        );
                    }
                    // v0.2.1 修复 D-1：反斜杠不再被视为转义字符，按普通字符处理
                    i += 1;
                }
            }
            _ => {}
        }
    }
}

/// Fuzz SQL 注入尝试：验证恶意输入不会突破字符串字面量
#[test]
fn fuzz_sql_injection_resistance() {
    let mut rng = Rng::new(123);
    let payloads = [
        "'; DROP TABLE users; --",
        "1' OR '1'='1",
        "'; EXEC xp_cmdshell('dir'); --",
        "' UNION SELECT * FROM passwords --",
        "Robert'); DROP TABLE students; --",
        "1'; DELETE FROM users WHERE '1'='1",
        "' OR ''='",
        "\"; DELETE FROM x; --",
        "'; UPDATE users SET admin=1; --",
        "\\'; DROP TABLE x; --",
        "null' OR 'x'='x",
        "1 OR 1=1--",
        "' OR 1=1#",
        "'; SHUTDOWN; --",
    ];

    for payload in &payloads {
        let v = Value::String(payload.to_string());
        let param = v.to_param().to_string();
        // 验证：生成的 param 是一个完整的字符串字面量
        // 计算 param 中单引号数量（考虑转义）
        let quote_count = count_unescaped_quotes(&param);
        assert!(
            quote_count.is_multiple_of(2),
            "Unbalanced quotes in SQL injection test: {} -> {}",
            payload,
            param
        );
    }

    // 随机生成更多注入尝试
    for _ in 0..FUZZ_ITERATIONS {
        let payload = generate_injection_payload(&mut rng);
        let v = Value::String(payload.clone());
        let param = v.to_param().to_string();
        let quote_count = count_unescaped_quotes(&param);
        assert_eq!(
            quote_count % 2,
            0,
            "Unbalanced quotes for payload: {:?} -> {}",
            payload,
            param
        );
    }
}

/// Fuzz MySQL escape_string：验证转义后不包含原始的未转义特殊字符
#[test]
fn fuzz_mysql_escape_string() {
    let mut rng = Rng::new(456);
    let dialect = MySqlDialect;
    for _ in 0..FUZZ_ITERATIONS {
        let len = rng.next_usize(100) + 1;
        let s = rng.next_string(len);
        let escaped = dialect.escape_string(&s);
        // 验证：转义后的字符串中不包含未转义的特殊字符
        // 反斜杠必须成对出现（除非是转义序列的一部分）
        let mut i = 0;
        let chars: Vec<char> = escaped.chars().collect();
        while i < chars.len() {
            if chars[i] == '\'' {
                // 单引号必须被反斜杠转义
                assert!(
                    i > 0 && chars[i - 1] == '\\',
                    "Unescaped single quote in MySQL escape: {:?} -> {}",
                    s,
                    escaped
                );
            }
            i += 1;
        }
    }
}

/// Fuzz PostgreSQL escape_string：验证使用双单引号转义
#[test]
fn fuzz_postgresql_escape_string() {
    let mut rng = Rng::new(789);
    let dialect = PostgreSqlDialect;
    for _ in 0..FUZZ_ITERATIONS {
        let len = rng.next_usize(100) + 1;
        let s = rng.next_string(len);
        let escaped = dialect.escape_string(&s);
        // 验证：PG 转义后单引号必须成对（''）
        let mut i = 0;
        let chars: Vec<char> = escaped.chars().collect();
        while i < chars.len() {
            if chars[i] == '\'' {
                // 必须有下一个字符且也是单引号
                assert!(
                    i + 1 < chars.len() && chars[i + 1] == '\'',
                    "Single quote not doubled in PG escape: {:?} -> {}",
                    s,
                    escaped
                );
                i += 2; // 跳过这对
            } else {
                i += 1;
            }
        }
    }
}

/// Fuzz SQLite escape_string
#[test]
fn fuzz_sqlite_escape_string() {
    let mut rng = Rng::new(101);
    let dialect = SqliteDialect;
    for _ in 0..FUZZ_ITERATIONS {
        let len = rng.next_usize(100) + 1;
        let s = rng.next_string(len);
        let escaped = dialect.escape_string(&s);
        // SQLite 使用双单引号转义
        let mut i = 0;
        let chars: Vec<char> = escaped.chars().collect();
        while i < chars.len() {
            if chars[i] == '\'' {
                assert!(
                    i + 1 < chars.len() && chars[i + 1] == '\'',
                    "Single quote not doubled in SQLite escape: {:?} -> {}",
                    s,
                    escaped
                );
                i += 2;
            } else {
                i += 1;
            }
        }
    }
}

/// Fuzz json_extract：验证生成的 SQL 是合法的
#[test]
fn fuzz_json_extract() {
    let mut rng = Rng::new(202);
    let dialects: Vec<Box<dyn Dialect>> = vec![
        Box::new(MySqlDialect),
        Box::new(PostgreSqlDialect),
        Box::new(SqliteDialect),
    ];

    for dialect in &dialects {
        let style = EscapeStyle::from_db_type(dialect.db_type());
        for _ in 0..200 {
            let path = generate_random_json_path(&mut rng);
            let sql = dialect.json_extract("data", &path);
            // 验证：SQL 不为空
            assert!(
                !sql.is_empty(),
                "json_extract returned empty for path: {}",
                path
            );
            // 验证：SQL 结构平衡（括号 + 字符串字面量）
            assert!(
                is_balanced(&sql, '(', ')', style),
                "Unbalanced parens: {}",
                sql
            );
            assert!(
                is_string_closed(&sql, style),
                "Unclosed string literal: {}",
                sql
            );
        }
    }
}

/// Fuzz query builder：随机构建查询，验证 SQL 合法性
#[test]
fn fuzz_query_builder() {
    let mut rng = Rng::new(303);
    let dialect = get_dialect(DbType::MySQL).unwrap();

    for i in 0..FUZZ_ITERATIONS {
        let mut builder = QueryBuilder::<FuzzModel>::new(get_dialect(DbType::MySQL).unwrap());
        builder = builder.table("users");

        // 随机选择列
        if rng.next_bool() {
            let cols = vec!["id", "name", "email"];
            builder = builder.select(cols);
        }

        // 随机添加 WHERE 条件
        let condition_count = rng.next_usize(5);
        for _ in 0..condition_count {
            let field = match rng.next_usize(3) {
                0 => "id",
                1 => "name",
                _ => "email",
            };
            match rng.next_usize(6) {
                0 => {
                    let val = rng.next_i64();
                    builder = builder.where_cond(format!("{} = {}", field, val));
                }
                1 => {
                    // 用 Value::String + where_in 让 query builder 自动转义，避免手动拼接错误
                    let val = rng.next_string(10);
                    builder = builder.where_in(field, vec![Value::String(val)]);
                }
                2 => {
                    let vals: Vec<Value> = (0..3).map(|_| Value::I64(rng.next_i64())).collect();
                    builder = builder.where_in(field, vals);
                }
                3 => {
                    builder = builder.where_null(field);
                }
                4 => {
                    builder = builder.where_not_null(field);
                }
                _ => {
                    builder = builder.or_where(format!("{} > {}", field, rng.next_i64()));
                }
            }
        }

        // 随机构建不同类型的 SQL
        let sql = match i % 6 {
            0 => builder.build_select(),
            1 => builder.build_count(),
            2 => builder.build_exists(),
            3 => builder.build_delete(),
            4 => builder.build_max("id"),
            _ => builder.build_min("id"),
        };

        // 验证：SQL 不为空
        assert!(!sql.is_empty(), "Empty SQL generated");
        // 验证：括号平衡
        // v0.2.1 修复 D-1：Value::to_param 只用 '' 转义，不再使用反斜杠转义，
        // 所以用 DoubleQuote 风格检查（反斜杠视为普通字符）
        assert!(
            is_balanced(&sql, '(', ')', EscapeStyle::DoubleQuote),
            "Unbalanced parens: {}",
            sql
        );
        // 验证：字符串字面量正确闭合
        assert!(
            is_string_closed(&sql, EscapeStyle::DoubleQuote),
            "Unclosed string literal: {}",
            sql
        );
    }

    let _ = dialect; // 避免未使用警告
}

/// Fuzz build_alter_table：随机生成 ALTER TABLE 语句
#[test]
fn fuzz_build_alter_table() {
    let mut rng = Rng::new(404);
    let dialects: Vec<Box<dyn Dialect>> = vec![
        Box::new(MySqlDialect),
        Box::new(PostgreSqlDialect),
        Box::new(SqliteDialect),
    ];

    for dialect in &dialects {
        let style = EscapeStyle::from_db_type(dialect.db_type());
        for _ in 0..200 {
            let changes = generate_random_table_changes(&mut rng);
            let sql = dialect.build_alter_table("test_table", &changes);
            assert!(!sql.is_empty());
            // 验证括号平衡（识别字符串字面量）
            assert!(
                is_balanced(&sql, '(', ')', style),
                "Unbalanced parens: {}",
                sql
            );
            // 验证字符串字面量正确闭合
            assert!(
                is_string_closed(&sql, style),
                "Unclosed string literal: {}",
                sql
            );
        }
    }
}

/// Fuzz pagination 边界：page=0, limit=0, 大数
#[test]
fn fuzz_pagination_boundaries() {
    let dialect = MySqlDialect;
    let base_sql = "SELECT * FROM users";

    // 边界值
    let boundary_values = [0u64, 1, 100, u64::MAX, u64::MAX - 1];
    for &page in &boundary_values {
        for &limit in &boundary_values {
            let sql = dialect.build_pagination(base_sql, page, limit);
            // 验证：始终包含 LIMIT 和 OFFSET
            assert!(sql.contains("LIMIT"), "Missing LIMIT: {}", sql);
            assert!(sql.contains("OFFSET"), "Missing OFFSET: {}", sql);
        }
    }
}

/// Fuzz Value 类型转换：验证不会 panic
#[test]
fn fuzz_value_type_conversions() {
    let mut rng = Rng::new(505);
    for _ in 0..FUZZ_ITERATIONS {
        let v = generate_random_value(&mut rng);
        // 这些转换不应 panic
        let _ = v.as_i64();
        let _ = v.as_f64();
        let _ = v.as_bool();
        let _ = v.as_str();
        let _ = v.as_bytes();
        let _ = v.is_null();
        let _ = v.is_bool();
        let _ = v.is_i64();
        let _ = v.is_f64();
        let _ = v.is_string();
        let _ = v.is_bytes();
        let _ = format!("{}", v);
        let _ = format!("{:?}", v);
    }
}

/// Fuzz ModelExt fill：验证 guarded 字段被过滤
#[test]
fn fuzz_model_fill_guarded() {
    struct User {
        id: i64,
        name: String,
        email: String,
        is_admin: bool,
    }
    impl Model for User {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "users"
        }
        fn pk(&self) -> Self::PrimaryKey {
            self.id
        }
        fn set_pk(&mut self, pk: Self::PrimaryKey) {
            self.id = pk;
        }
    }
    impl ModelExt for User {
        fn columns() -> Vec<&'static str> {
            vec!["id", "name", "email", "is_admin"]
        }
        fn fillable() -> Vec<&'static str> {
            vec!["name", "email"]
        }
        fn guarded() -> Vec<&'static str> {
            vec!["id", "is_admin"]
        }
        fn get_column_value(&self, col: &str) -> Option<Value> {
            match col {
                "id" => Some(Value::I64(self.id)),
                "name" => Some(Value::String(self.name.clone())),
                "email" => Some(Value::String(self.email.clone())),
                "is_admin" => Some(Value::Bool(self.is_admin)),
                _ => None,
            }
        }
        fn from_value(&mut self, map: std::collections::HashMap<String, Value>) {
            if let Some(Value::I64(id)) = map.get("id") {
                self.id = *id;
            }
            if let Some(Value::String(s)) = map.get("name") {
                self.name = s.clone();
            }
            if let Some(Value::String(s)) = map.get("email") {
                self.email = s.clone();
            }
            if let Some(Value::Bool(b)) = map.get("is_admin") {
                self.is_admin = *b;
            }
        }
    }

    let mut rng = Rng::new(606);
    for _ in 0..200 {
        let mut user = User {
            id: 100,
            name: "original".to_string(),
            email: "orig@test.com".to_string(),
            is_admin: false,
        };
        let mut data = std::collections::HashMap::new();
        // 尝试注入 guarded 字段
        data.insert("id".to_string(), Value::I64(rng.next_i64()));
        data.insert("is_admin".to_string(), Value::Bool(true));
        data.insert("name".to_string(), Value::String(rng.next_string(10)));
        data.insert("email".to_string(), Value::String(rng.next_string(10)));

        user.fill(data);
        // 验证：id 和 is_admin 不应被修改
        assert_eq!(user.id, 100, "guarded field id was modified");
        assert!(!user.is_admin, "guarded field is_admin was modified");
    }
}

// ===== 辅助函数 =====

fn generate_random_value(rng: &mut Rng) -> Value {
    match rng.next_usize(16) {
        0 => Value::Null,
        1 => Value::Bool(rng.next_bool()),
        2 => Value::I8(rng.next_i64() as i8),
        3 => Value::I16(rng.next_i64() as i16),
        4 => Value::I32(rng.next_i64() as i32),
        5 => Value::I64(rng.next_i64()),
        6 => Value::U8(rng.next_u64() as u8),
        7 => Value::U16(rng.next_u64() as u16),
        8 => Value::U32(rng.next_u64() as u32),
        9 => Value::U64(rng.next_u64()),
        10 => Value::F32(rng.next_f64() as f32),
        11 => Value::F64(rng.next_f64()),
        12 => {
            let len = rng.next_usize(50) + 1;
            Value::String(rng.next_string(len))
        }
        13 => {
            let len = rng.next_usize(30) + 1;
            Value::Bytes(rng.next_bytes(len))
        }
        14 => Value::Date(format!(
            "2026-{:02}-{:02}",
            rng.next_usize(12) + 1,
            rng.next_usize(28) + 1
        )),
        15 => Value::Json(format!("{{\"key\":\"{}\"}}", rng.next_string(10))),
        _ => Value::Uuid(format!(
            "{}-{}-{}-{}",
            rng.next_u64(),
            rng.next_u64(),
            rng.next_u64(),
            rng.next_u64()
        )),
    }
}

fn generate_injection_payload(rng: &mut Rng) -> String {
    let templates = [
        "'; DROP TABLE x; --",
        "' OR '1'='1",
        "1' OR 1=1; --",
        "'; EXEC('x'); --",
        "' UNION SELECT 1; --",
        "\\''; DELETE FROM y; --",
        "'; SHUTDOWN; --",
        "' AND 1=1; --",
        "'; INSERT INTO z VALUES(1); --",
    ];
    let base = templates[rng.next_usize(templates.len())];
    let suffix_len = rng.next_usize(20);
    let suffix = rng.next_string(suffix_len);
    format!("{}{}", base, suffix)
}

fn generate_random_json_path(rng: &mut Rng) -> String {
    let depth = rng.next_usize(5) + 1;
    let mut path = String::new();
    if rng.next_bool() {
        path.push_str("$.");
    }
    for i in 0..depth {
        if i > 0 {
            path.push('.');
        }
        let len = rng.next_usize(8) + 1;
        // 排除单引号，避免方言间转义差异（MySQL 用 \'，PG/SQLite 用 ''）
        // 单独的测试用例覆盖单引号场景
        let raw = rng.next_string(len);
        let cleaned: String = raw.chars().filter(|c| *c != '\'' && *c != '\0').collect();
        path.push_str(&cleaned.replace('.', "_"));
    }
    path
}

fn generate_random_table_changes(rng: &mut Rng) -> Vec<TableChange> {
    let mut changes = Vec::new();
    let count = rng.next_usize(5) + 1;
    for _ in 0..count {
        let col_def = ColumnDef {
            name: format!("col_{}", rng.next_usize(100)),
            sql_type: match rng.next_usize(4) {
                0 => "INT".to_string(),
                1 => "VARCHAR(255)".to_string(),
                2 => "TEXT".to_string(),
                _ => "BOOLEAN".to_string(),
            },
            nullable: rng.next_bool(),
            default: if rng.next_bool() {
                Some("0".to_string())
            } else {
                None
            },
            auto_increment: false,
            primary_key: false,
        };
        match rng.next_usize(6) {
            0 => changes.push(TableChange::AddColumn(col_def)),
            1 => changes.push(TableChange::DropColumn(format!(
                "col_{}",
                rng.next_usize(100)
            ))),
            2 => changes.push(TableChange::ModifyColumn(col_def)),
            3 => changes.push(TableChange::AddIndex(
                format!("idx_{}", rng.next_usize(100)),
                vec![format!("col_{}", rng.next_usize(100))],
            )),
            4 => changes.push(TableChange::DropIndex(format!(
                "idx_{}",
                rng.next_usize(100)
            ))),
            _ => changes.push(TableChange::AddForeignKey {
                columns: vec![format!("col_{}", rng.next_usize(100))],
                reference_table: format!("ref_{}", rng.next_usize(100)),
                reference_columns: vec!["id".to_string()],
            }),
        }
    }
    changes
}

fn count_unescaped_quotes(s: &str) -> usize {
    // v0.2.1 修复 D-1：escape_string 只用 '' 转义，不再使用反斜杠转义
    // 所以只检查 '' 转义模式，不跳过反斜杠后的字符
    let mut count = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.peek() == Some(&'\'') {
                chars.next(); // 跳过 '' 转义
            } else {
                count += 1; // 未转义的 '
            }
        }
    }
    count
}

/// SQL 字符串字面量的转义风格
#[derive(Clone, Copy, PartialEq)]
enum EscapeStyle {
    /// MySQL 风格：反斜杠是转义字符（\' \\ \n \r \t \0 \Z）
    Backslash,
    /// PG/SQLite 风格：反斜杠不是转义字符，只有 '' 是转义
    DoubleQuote,
}

impl EscapeStyle {
    fn from_db_type(db_type: DbType) -> Self {
        match db_type {
            DbType::MySQL => EscapeStyle::Backslash,
            _ => EscapeStyle::DoubleQuote,
        }
    }
}

fn is_balanced(s: &str, open: char, close: char, style: EscapeStyle) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            match style {
                EscapeStyle::Backslash => {
                    // MySQL 风格：反斜杠转义下一字符
                    if c == '\\' {
                        chars.next();
                        continue;
                    }
                    if c == '\'' {
                        // 同时识别 '' 转义（MySQL 也支持）
                        if chars.peek() == Some(&'\'') {
                            chars.next();
                        } else {
                            in_string = false;
                        }
                    }
                }
                EscapeStyle::DoubleQuote => {
                    // PG/SQLite 风格：反斜杠不是转义字符，只有 '' 是转义
                    if c == '\'' {
                        if chars.peek() == Some(&'\'') {
                            chars.next();
                        } else {
                            in_string = false;
                        }
                    }
                }
            }
            continue;
        }
        if c == '\'' {
            in_string = true;
        } else if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
    }
    depth == 0
}

/// 验证 SQL 中所有单引号字符串字面量是否正确闭合
/// 识别 '' 转义和（可选的）\ 转义
fn is_string_closed(s: &str, style: EscapeStyle) -> bool {
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            match style {
                EscapeStyle::Backslash => {
                    if c == '\\' {
                        chars.next(); // 反斜杠转义：跳过下一个字符
                        continue;
                    }
                    if c == '\'' {
                        if chars.peek() == Some(&'\'') {
                            chars.next(); // '' 转义
                        } else {
                            in_string = false;
                        }
                    }
                }
                EscapeStyle::DoubleQuote => {
                    if c == '\'' {
                        if chars.peek() == Some(&'\'') {
                            chars.next(); // '' 转义
                        } else {
                            in_string = false;
                        }
                    }
                }
            }
            continue;
        }
        if c == '\'' {
            in_string = true;
        }
    }
    !in_string
}
