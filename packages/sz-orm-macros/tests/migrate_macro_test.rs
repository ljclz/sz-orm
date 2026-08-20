use sz_orm_core::migration::{MigrationContext, Migrator};
use sz_orm_macros::migrate;

#[test]
fn test_migrate_macro_creates_migration() {
    let m = migrate!(
        "001",
        "create_users",
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "DROP TABLE users"
    );

    assert_eq!(m.version, "001");
    assert_eq!(m.name, "create_users");
    assert!(m.sql_up.contains("CREATE TABLE users"));
    assert!(m.sql_down.contains("DROP TABLE users"));
    assert_eq!(m.batch, 0);
    assert!(m.executed_at.is_none());
}

#[test]
fn test_e2e_migrate_macro_with_migrator() {
    let m1 = migrate!(
        "001",
        "create_users",
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "DROP TABLE users"
    );
    let m2 = migrate!(
        "002",
        "add_email_column",
        "ALTER TABLE users ADD COLUMN email TEXT",
        "ALTER TABLE users DROP COLUMN email"
    );
    let m3 = migrate!(
        "003",
        "create_orders",
        "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id))",
        "DROP TABLE orders"
    );

    let migrator = Migrator::new(MigrationContext::default())
        .add_migration(m1)
        .add_migration(m2)
        .add_migration(m3);

    assert_eq!(migrator.get_migrations().len(), 3);
    assert_eq!(migrator.get_pending_migrations().len(), 3);
    assert_eq!(migrator.get_applied_migrations().len(), 0);
    assert_eq!(migrator.latest_version(), Some("003"));

    let found = migrator.find_migration("002").unwrap();
    assert_eq!(found.name, "add_email_column");
    assert!(found.sql_up.contains("ALTER TABLE users ADD COLUMN email"));
    assert!(found.sql_down.contains("DROP COLUMN email"));
}

#[test]
fn test_e2e_migrate_macro_add_migrations_batch() {
    let migrations = vec![
        migrate!("001", "init", "CREATE TABLE t1 (id INT)", "DROP TABLE t1"),
        migrate!("002", "add_t2", "CREATE TABLE t2 (id INT)", "DROP TABLE t2"),
    ];

    let migrator = Migrator::new(MigrationContext::default()).add_migrations(migrations);

    assert_eq!(migrator.get_migrations().len(), 2);
    assert_eq!(migrator.latest_version(), Some("002"));

    let m = migrator.find_migration("001").unwrap();
    assert_eq!(m.name, "init");
    assert!(m.sql_up.contains("CREATE TABLE t1"));
}

#[test]
fn test_e2e_migrate_macro_complex_ddl() {
    let m = migrate!(
        "20240101",
        "create_products_with_index",
        "CREATE TABLE products (id BIGINT PRIMARY KEY, name VARCHAR(255) NOT NULL, price DECIMAL(10,2)); CREATE INDEX idx_products_name ON products(name)",
        "DROP INDEX idx_products_name; DROP TABLE products"
    );

    assert_eq!(m.version, "20240101");
    assert!(m.sql_up.contains("CREATE TABLE products"));
    assert!(m.sql_up.contains("CREATE INDEX idx_products_name"));
    assert!(m.sql_down.contains("DROP INDEX"));
    assert!(m.sql_down.contains("DROP TABLE products"));

    let migrator = Migrator::new(MigrationContext::default()).add_migration(m);
    assert_eq!(migrator.get_pending_migrations().len(), 1);
}
