//! Db2 独立方言集成测试
//!
//! Db2Dialect 是独立方言实现，使用双引号 quote、
//! OFFSET ... FETCH FIRST ... ROWS ONLY 分页、IDENTITY 列、SMALLINT 替代 BOOLEAN。
//!
//! 受限说明：Db2 真实 DB 集成测试需要 ODBC 驱动或专用 `ibm-db2` crate。
//! 受项目"不引入新 Rust 依赖"约束，当前仅提供方言断言测试（10 个）。
//!
//! 后续扩展路径：
//! - 当项目引入 `ibm-db2` 或 ODBC 驱动后，补充真实 DB 测试
//! - 使用 `"user"` 双引号 quote、`?` 占位符、`FETCH FIRST n ROWS ONLY` 分页验证
//! - Db2 Docker 容器部署：`docker run -d ibmcom/db2`（需接受许可证协议）
//! - 环境变量：SZ_ORM_DB2_URL 配置连接参数
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_db2

use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

fn test_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ]
}

// ==================== 方言断言测试 ====================

#[test]
fn test_db2_dialect_quote() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert_eq!(dialect.quote("user"), "\"user\"");
}

#[test]
fn test_db2_dialect_db_type() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert_eq!(dialect.db_type(), DbType::Db2);
}

#[test]
fn test_db2_dialect_pagination() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let sql = dialect.build_pagination("SELECT 1", 10, 20);
    assert!(
        sql.contains("FETCH FIRST") || sql.contains("OFFSET"),
        "Db2 pagination must use OFFSET/FETCH FIRST: got {}",
        sql
    );
}

#[test]
fn test_db2_dialect_create_table() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let sql = dialect.build_create_table("test_table", &test_columns());
    assert!(sql.contains("CREATE TABLE"), "Must contain CREATE TABLE");
    assert!(
        sql.contains("\"test_table\""),
        "Must quote table name with double quotes"
    );
}

#[test]
fn test_db2_dialect_no_returning() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert!(
        !dialect.supports_returning(),
        "Db2 does not support RETURNING"
    );
}

#[test]
fn test_db2_dialect_supports_lock() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    assert!(
        dialect.supports_lock_for_update(),
        "Db2 supports FOR UPDATE"
    );
}

#[test]
fn test_db2_dialect_type_mapping_bigint() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "BIGINT".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(sql.contains("BIGINT"), "BIGINT should stay BIGINT in Db2");
}

#[test]
fn test_db2_dialect_type_mapping_int() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "INT".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: true,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(sql.contains("INTEGER"), "INT should map to INTEGER in Db2");
}

#[test]
fn test_db2_dialect_type_mapping_boolean() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "flag".to_string(),
        sql_type: "BOOLEAN".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: false,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(
        sql.contains("SMALLINT"),
        "BOOLEAN should map to SMALLINT in Db2"
    );
}

#[test]
fn test_db2_dialect_type_mapping_text() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "content".to_string(),
        sql_type: "TEXT".to_string(),
        nullable: true,
        default: None,
        auto_increment: false,
        primary_key: false,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(sql.contains("CLOB"), "TEXT should map to CLOB in Db2");
}

#[test]
fn test_db2_dialect_type_mapping_datetime() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let cols = vec![ColumnDef {
        name: "ts".to_string(),
        sql_type: "DATETIME".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: false,
    }];
    let sql = dialect.build_create_table("t", &cols);
    assert!(
        sql.contains("TIMESTAMP"),
        "DATETIME should map to TIMESTAMP in Db2"
    );
}

// ============================================================================
// 容器化方言矩阵（2026-11-13，MZ-DB2 × DB2 Community 容器 @55000，CLP 脚本通道）
// 与既有 11 项不重复：聚焦真库行为（SYSDUMMY1 / OFFSET..FETCH / 大写折叠 / IDENTITY 回填）
// 说明：DB2 无 Rust 驱动（项目约束不引入 ibm-db2/ODBC），矩阵经容器内 CLP 脚本执行，
// 进程内断言仅限方言生成面；真库行为断言以 CLP 输出为准（报告附 evidence）。
// ============================================================================

/// MZ-DB2-1: FROM-less 探针 SYSIBM.SYSDUMMY1（方言 last_insert_id_sql 依赖该通道）
#[test]
fn matrix_mz_db2_sysdummy1_channel() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    // 方言生成面：last_insert_id_sql 走 SYSIBM.SYSDUMMY1
    assert_eq!(
        dialect.last_insert_id_sql(),
        Some("SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1"),
        "Db2 主键回填通道必须经 SYSIBM.SYSDUMMY1（FROM-less 标准形）"
    );
    // 真库证据（容器 CLP 实测 2026-11-13）：
    //   SELECT * FROM SYSIBM.SYSDUMMY1 → 1 row (IBMREQD=Y)
    //   SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1 → IDENTITY 回填值
    //   见 docs/assessment/2026-11-13-container-dialect-matrix.md MZ-DB2 节
}

/// MZ-DB2-2: 分页语法 OFFSET..FETCH NEXT（sz-orm 生成面与 DB2 LUW 11.5+ 真库一致性）
#[test]
fn matrix_mz_db2_pagination_offset_fetch() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    let sql = dialect.build_pagination("SELECT id FROM t ORDER BY id", 2, 2);
    assert_eq!(
        sql,
        "SELECT id FROM t ORDER BY id OFFSET 2 ROWS FETCH NEXT 2 ROWS ONLY"
    );
    // 真库证据（容器 CLP 实测 2026-11-13，mzp 表 2 行）：
    //   SELECT id FROM mzp ORDER BY id OFFSET 1 ROWS FETCH NEXT 1 ROWS ONLY → id=2（1 row）
    //   SELECT id FROM mzp ORDER BY id DESC OFFSET 1 ROWS FETCH NEXT 1 ROWS ONLY → id=1（1 row）
    //   DB2 LUW 11.5+ 支持 OFFSET..FETCH NEXT（与 sz-orm 生成面一致）
}

/// MZ-DB2-3: 标识符大写折叠——双引号 quoting 阻止折叠（同 Oracle 向）
#[test]
fn matrix_mz_db2_identifier_folding() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    // 方言生成面：双引号 quote
    assert_eq!(dialect.quote("order"), "\"order\"");
    // 真库证据（容器 CLP 实测 2026-11-13）：
    //   CREATE TABLE mzcase (id INT, "order" VARCHAR(20)) 成功
    //   SELECT "order" FROM mzcase → 'o'（1 row）——双引号保留小写、且保留字可作列名
    //   未 quote 的裸标识符按大写折叠存储（DB2 目录大写）
}

/// MZ-DB2-4: IDENTITY 回填双通道——IDENTITY_VAL_LOCAL()（sz-orm 通道）+ NEW TABLE（引擎扩展）
#[test]
fn matrix_mz_db2_identity_backfill_channels() {
    let dialect = get_dialect(DbType::Db2).unwrap();
    // sz-orm 生成面：auto_increment_keyword = GENERATED ALWAYS AS IDENTITY
    assert_eq!(
        dialect.auto_increment_keyword(),
        "GENERATED ALWAYS AS IDENTITY"
    );
    // 真库证据（容器 CLP 实测 2026-11-13，mzcap 表）：
    //   GENERATED ALWAYS AS IDENTITY 建表 + INSERT（无 id）成功
    //   SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1 → 1.（sz-orm 回填通道可用）
    //   SELECT id FROM NEW TABLE (INSERT ...) → 2（引擎扩展回填通道，sz-orm 未采用——登记）
}
