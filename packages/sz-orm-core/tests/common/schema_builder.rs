//! 测试数据构造器（v2.4.0 任务 1.4）
//!
//! 提供五方言通用的测试 Schema 构造、数据填充与清理工具。
//! 建表：users / orders / profiles / roles / user_roles 五表。
//! 数据：5 users + 10 orders + 3 profiles + 3 roles + 6 user_roles，覆盖边界情况。

use std::collections::HashMap;

use sz_orm_core::Value;

/// 测试方言标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestDialect {
    MySql,
    Postgres,
    Sqlite,
    Oracle,
    MsSql,
}

impl TestDialect {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestDialect::MySql => "mysql",
            TestDialect::Postgres => "postgres",
            TestDialect::Sqlite => "sqlite",
            TestDialect::Oracle => "oracle",
            TestDialect::MsSql => "mssql",
        }
    }
}

/// 测试 Schema 构造器A
///
/// 按方言生成 DDL 建表（users/orders/profiles/roles/user_roles 五表），
/// 提供数据填充与清理方法。
pub struct TestSchemaBuilder {
    dialect: TestDialect,
}

impl TestSchemaBuilder {
    pub fn new(dialect: TestDialect) -> Self {
        Self { dialect }
    }

    /// 生成五表的 DDL 语句列表
    ///
    /// 方言感知：MySQL/PG/SQLite/Oracle/MSSQL 各自 DDL 语法。
    /// 返回按依赖顺序排列的 DDL（被引用表在前）。
    pub fn build_ddl(&self) -> Vec<String> {
        let id_type = match self.dialect {
            TestDialect::Oracle => "NUMBER(19)".to_string(),
            TestDialect::MsSql => "BIGINT".to_string(),
            _ => "BIGINT".to_string(),
        };
        let text_type = match self.dialect {
            TestDialect::MySql => "VARCHAR(255)".to_string(),
            TestDialect::Postgres => "TEXT".to_string(),
            TestDialect::Sqlite => "TEXT".to_string(),
            TestDialect::Oracle => "VARCHAR2(255)".to_string(),
            TestDialect::MsSql => "NVARCHAR(255)".to_string(),
        };
        let auto_id = match self.dialect {
            TestDialect::MySql => "BIGINT AUTO_INCREMENT PRIMARY KEY".to_string(),
            TestDialect::Postgres => "BIGSERIAL PRIMARY KEY".to_string(),
            TestDialect::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT".to_string(),
            TestDialect::Oracle => {
                "NUMBER(19) GENERATED ALWAYS AS IDENTITY PRIMARY KEY".to_string()
            }
            TestDialect::MsSql => "BIGINT IDENTITY(1,1) PRIMARY KEY".to_string(),
        };

        vec![
            format!("CREATE TABLE IF NOT EXISTS users (id {auto_id}, name {text_type}, email {text_type})"),
            format!("CREATE TABLE IF NOT EXISTS profiles (id {auto_id}, user_id {id_type}, bio {text_type})"),
            format!("CREATE TABLE IF NOT EXISTS orders (id {auto_id}, user_id {id_type}, amount {id_type}, status {text_type})"),
            format!("CREATE TABLE IF NOT EXISTS roles (id {auto_id}, name {text_type})"),
            format!("CREATE TABLE IF NOT EXISTS user_roles (id {auto_id}, user_id {id_type}, role_id {id_type})"),
        ]
    }

    /// 生成清理 DDL（DROP TABLE IF EXISTS）
    ///
    /// 按反向依赖顺序排列（先删引用表，再删被引用表）。
    pub fn teardown_ddl(&self) -> Vec<String> {
        let drop_if_exists = |table: &str| -> String {
            match self.dialect {
                TestDialect::Oracle => format!("DROP TABLE IF EXISTS {table}"),
                _ => format!("DROP TABLE IF EXISTS {table}"),
            }
        };
        vec![
            drop_if_exists("user_roles"),
            drop_if_exists("orders"),
            drop_if_exists("profiles"),
            drop_if_exists("roles"),
            drop_if_exists("users"),
        ]
    }

    /// 生成测试数据 INSERT 语句列表
    ///
    /// 数据覆盖边界情况：
    /// - user3：空关联（无 orders/profiles/roles）
    /// - user4：单条关联（1 order, 1 profile）
    /// - user1/user5：多条关联（多 orders, 多 roles）
    pub fn seed_data(&self) -> Vec<String> {
        vec![
            // users: 5 条
            "INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@test.com')".to_string(),
            "INSERT INTO users (id, name, email) VALUES (2, 'Bob', 'bob@test.com')".to_string(),
            "INSERT INTO users (id, name, email) VALUES (3, 'Carol', 'carol@test.com')".to_string(),
            "INSERT INTO users (id, name, email) VALUES (4, 'Dave', 'dave@test.com')".to_string(),
            "INSERT INTO users (id, name, email) VALUES (5, 'Eve', 'eve@test.com')".to_string(),
            // profiles: 3 条（user1, user2, user4 各一条，user3/user5 无 profile）
            "INSERT INTO profiles (id, user_id, bio) VALUES (1, 1, 'Alice bio')".to_string(),
            "INSERT INTO profiles (id, user_id, bio) VALUES (2, 2, 'Bob bio')".to_string(),
            "INSERT INTO profiles (id, user_id, bio) VALUES (3, 4, 'Dave bio')".to_string(),
            // orders: 10 条（user1: 4, user2: 3, user4: 1, user5: 2, user3: 0）
            "INSERT INTO orders (id, user_id, amount, status) VALUES (1, 1, 100, 'paid')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (2, 1, 200, 'paid')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (3, 1, 300, 'pending')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (4, 1, 50, 'cancelled')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (5, 2, 150, 'paid')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (6, 2, 250, 'pending')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (7, 2, 350, 'paid')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (8, 4, 400, 'paid')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (9, 5, 500, 'pending')"
                .to_string(),
            "INSERT INTO orders (id, user_id, amount, status) VALUES (10, 5, 600, 'paid')"
                .to_string(),
            // roles: 3 条
            "INSERT INTO roles (id, name) VALUES (1, 'admin')".to_string(),
            "INSERT INTO roles (id, name) VALUES (2, 'user')".to_string(),
            "INSERT INTO roles (id, name) VALUES (3, 'guest')".to_string(),
            // user_roles: 6 条（user1: admin+user, user2: user, user4: user+guest, user5: admin）
            "INSERT INTO user_roles (id, user_id, role_id) VALUES (1, 1, 1)".to_string(),
            "INSERT INTO user_roles (id, user_id, role_id) VALUES (2, 1, 2)".to_string(),
            "INSERT INTO user_roles (id, user_id, role_id) VALUES (3, 2, 2)".to_string(),
            "INSERT INTO user_roles (id, user_id, role_id) VALUES (4, 4, 2)".to_string(),
            "INSERT INTO user_roles (id, user_id, role_id) VALUES (5, 4, 3)".to_string(),
            "INSERT INTO user_roles (id, user_id, role_id) VALUES (6, 5, 1)".to_string(),
        ]
    }

    /// 获取预期的测试数据统计
    pub fn expected_counts() -> TestSchemaCounts {
        TestSchemaCounts {
            users: 5,
            profiles: 3,
            orders: 10,
            roles: 3,
            user_roles: 6,
        }
    }
}

/// 测试数据统计
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestSchemaCounts {
    pub users: usize,
    pub profiles: usize,
    pub orders: usize,
    pub roles: usize,
    pub user_roles: usize,
}

/// 构造一行数据的辅助函数
pub fn make_row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ddl_all_dialects() {
        for dialect in [
            TestDialect::MySql,
            TestDialect::Postgres,
            TestDialect::Sqlite,
            TestDialect::Oracle,
            TestDialect::MsSql,
        ] {
            let builder = TestSchemaBuilder::new(dialect);
            let ddl = builder.build_ddl();
            assert_eq!(ddl.len(), 5, "{} 应生成 5 表 DDL", dialect.as_str());
            assert!(ddl[0].contains("users"), "第一张表应为 users");
            assert!(ddl[4].contains("user_roles"), "最后一张表应为 user_roles");
        }
    }

    #[test]
    fn test_teardown_ddl_reverse_order() {
        let builder = TestSchemaBuilder::new(TestDialect::MySql);
        let teardown = builder.teardown_ddl();
        assert_eq!(teardown.len(), 5);
        assert!(teardown[0].contains("user_roles"), "先删引用表 user_roles");
        assert!(teardown[4].contains("users"), "最后删被引用表 users");
    }

    #[test]
    fn test_seed_data_covers_boundary() {
        let builder = TestSchemaBuilder::new(TestDialect::MySql);
        let seed = builder.seed_data();
        let counts = TestSchemaBuilder::expected_counts();
        let user_count = seed
            .iter()
            .filter(|s| s.contains("INSERT INTO users"))
            .count();
        let order_count = seed
            .iter()
            .filter(|s| s.contains("INSERT INTO orders"))
            .count();
        let profile_count = seed
            .iter()
            .filter(|s| s.contains("INSERT INTO profiles"))
            .count();
        let role_count = seed
            .iter()
            .filter(|s| s.contains("INSERT INTO roles"))
            .count();
        let user_role_count = seed
            .iter()
            .filter(|s| s.contains("INSERT INTO user_roles"))
            .count();
        assert_eq!(user_count, counts.users);
        assert_eq!(order_count, counts.orders);
        assert_eq!(profile_count, counts.profiles);
        assert_eq!(role_count, counts.roles);
        assert_eq!(user_role_count, counts.user_roles);
    }

    #[test]
    fn test_seed_data_user3_no_relations() {
        let builder = TestSchemaBuilder::new(TestDialect::MySql);
        let seed = builder.seed_data();
        let user3_orders = seed
            .iter()
            .any(|s| s.contains("INSERT INTO orders") && s.contains("3, 3,"));
        let user3_profiles = seed
            .iter()
            .any(|s| s.contains("INSERT INTO profiles") && s.contains("3, 3,"));
        let user3_roles = seed
            .iter()
            .any(|s| s.contains("INSERT INTO user_roles") && s.contains("3, 3,"));
        assert!(!user3_orders, "user3 不应有 orders");
        assert!(!user3_profiles, "user3 不应有 profiles");
        assert!(!user3_roles, "user3 不应有 roles");
    }

    #[test]
    fn test_dialect_as_str() {
        assert_eq!(TestDialect::MySql.as_str(), "mysql");
        assert_eq!(TestDialect::Postgres.as_str(), "postgres");
        assert_eq!(TestDialect::Sqlite.as_str(), "sqlite");
        assert_eq!(TestDialect::Oracle.as_str(), "oracle");
        assert_eq!(TestDialect::MsSql.as_str(), "mssql");
    }

    #[test]
    fn test_make_row() {
        let row = make_row(&[
            ("id", Value::I64(1)),
            ("name", Value::String("a".to_string())),
        ]);
        assert_eq!(row.len(), 2);
        assert_eq!(row.get("id"), Some(&Value::I64(1)));
    }
}
