// Type-level test: verify index.d.ts matches the actual wasm-bindgen exports.
// This file is compiled with `tsc --noEmit` but never executed.
// If the .d.ts is wrong, compilation will fail.

import { JsWasmDatabase, JsQueryResult } from "../index";

// JsWasmDatabase constructor
const db = new JsWasmDatabase();

// createTable(sql: string): number
const created: number = db.createTable("CREATE TABLE users (id INT, name TEXT)");

// insert(sql: string, paramsJson: string): number
const inserted: number = db.insert(
    "INSERT INTO users (id, name) VALUES (?, ?)",
    '[1, "Alice"]'
);

// query(sql: string, paramsJson: string): JsQueryResult
const result: JsQueryResult = db.query("SELECT * FROM users", "[]");

// JsQueryResult getters
const rowsJson: string = result.rowsJson;
const affected: number = result.affected;

// update(sql: string, paramsJson: string): number
const updated: number = db.update(
    "UPDATE users SET name = ? WHERE id = ?",
    '["Bob", 1]'
);

// delete(sql: string, paramsJson: string): number
const deleted: number = db.delete("DELETE FROM users WHERE id = ?", "[1]");

// tableNames(): string[]
const names: string[] = db.tableNames();

// tableRowCount(table: string): number
const rowCount: number = db.tableRowCount("users");

// All variables used to avoid unused warnings
void [created, inserted, rowsJson, affected, updated, deleted, names, rowCount];