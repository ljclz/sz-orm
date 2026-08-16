use std::collections::HashMap;

use crate::design_ir::SchemaDesign;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAlgorithm {
    ForceDirected,
    Grid,
}

#[derive(serde::Serialize)]
struct ErJsonNode {
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(serde::Serialize)]
struct ErJsonEdge {
    from: String,
    to: String,
    cardinality: String,
}

#[derive(serde::Serialize)]
struct ErJson {
    nodes: Vec<ErJsonNode>,
    edges: Vec<ErJsonEdge>,
}

pub struct ErDiagramEditor {
    design: SchemaDesign,
    positions: HashMap<String, (f64, f64)>,
}

impl ErDiagramEditor {
    pub fn new(design: SchemaDesign) -> Self {
        let mut editor = Self {
            design,
            positions: HashMap::new(),
        };
        editor.layout(LayoutAlgorithm::Grid);
        editor
    }

    pub fn layout(&mut self, algorithm: LayoutAlgorithm) {
        match algorithm {
            LayoutAlgorithm::Grid | LayoutAlgorithm::ForceDirected => {
                let cols = (self.design.tables.len() as f64).sqrt().ceil() as usize;
                let cols = cols.max(1);
                for (i, table) in self.design.tables.iter().enumerate() {
                    let x = (i % cols) as f64 * 280.0;
                    let y = (i / cols) as f64 * 220.0;
                    self.positions.insert(table.name.clone(), (x, y));
                }
            }
        }
    }

    pub fn to_svg(&self) -> String {
        let mut svg = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800" style="font-family:monospace;font-size:12px">"#,
        );

        for table in &self.design.tables {
            let (x, y) = self
                .positions
                .get(&table.name)
                .copied()
                .unwrap_or((0.0, 0.0));
            let height = 40.0 + table.columns.len() as f64 * 18.0;

            svg.push_str(&format!(
                r#"<rect x="{}" y="{}" width="240" height="{}" fill="white" stroke="black" stroke-width="1.5"/>"#,
                x, y, height
            ));
            svg.push_str(&format!(
                r#"<text x="{}" y="{}" font-weight="bold" font-size="14">{}</text>"#,
                x + 10.0,
                y + 20.0,
                table.name
            ));
            svg.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="0.5"/>"#,
                x,
                y + 28.0,
                x + 240.0,
                y + 28.0
            ));

            for (i, col) in table.columns.iter().enumerate() {
                let cy = y + 42.0 + i as f64 * 18.0;
                let pk_marker = if col.is_primary_key { "PK " } else { "   " };
                svg.push_str(&format!(
                    r#"<text x="{}" y="{}">{}{}: {}</text>"#,
                    x + 10.0,
                    cy,
                    pk_marker,
                    col.name,
                    col.col_type.to_sql_type()
                ));
            }
        }

        for rel in &self.design.relations {
            let from = self.positions.get(&rel.from_table).copied();
            let to = self.positions.get(&rel.to_table).copied();
            if let (Some((fx, fy)), Some((tx, ty))) = (from, to) {
                svg.push_str(&format!(
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="blue" stroke-width="1.5" marker-end="url(#arrow)"/>"#,
                    fx + 240.0,
                    fy + 50.0,
                    tx,
                    ty + 50.0
                ));
                let mid_x = (fx + 240.0 + tx) / 2.0;
                let mid_y = (fy + 50.0 + ty + 50.0) / 2.0;
                svg.push_str(&format!(
                    r#"<text x="{}" y="{}" fill="blue" font-weight="bold">{}</text>"#,
                    mid_x,
                    mid_y,
                    rel.cardinality.as_str()
                ));
            }
        }

        svg.push_str(
            r#"<defs><marker id="arrow" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto"><path d="M0,0 L0,6 L9,3 z" fill="blue"/></marker></defs>"#,
        );
        svg.push_str("</svg>");
        svg
    }

    pub fn to_json(&self) -> serde_json::Value {
        let nodes: Vec<ErJsonNode> = self
            .design
            .tables
            .iter()
            .map(|t| {
                let (x, y) = self.positions.get(&t.name).copied().unwrap_or((0.0, 0.0));
                ErJsonNode {
                    id: t.name.clone(),
                    x,
                    y,
                    width: 240.0,
                    height: 40.0 + t.columns.len() as f64 * 18.0,
                }
            })
            .collect();

        let edges: Vec<ErJsonEdge> = self
            .design
            .relations
            .iter()
            .map(|r| ErJsonEdge {
                from: r.from_table.clone(),
                to: r.to_table.clone(),
                cardinality: r.cardinality.as_str().to_string(),
            })
            .collect();

        serde_json::to_value(ErJson { nodes, edges }).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_ir::*;

    fn sample_design_with_relation() -> SchemaDesign {
        SchemaDesign::new(Dialect::MySql)
            .with_table(DesignTable::new(
                "users",
                vec![
                    DesignColumn::new("id", ColumnType::BigInt).primary_key(),
                    DesignColumn::new("name", ColumnType::Varchar(Some(255))),
                ],
            ))
            .with_table(DesignTable::new(
                "orders",
                vec![
                    DesignColumn::new("id", ColumnType::BigInt).primary_key(),
                    DesignColumn::new("user_id", ColumnType::BigInt),
                ],
            ))
            .with_relation(DesignRelation {
                from_table: "users".to_string(),
                to_table: "orders".to_string(),
                from_column: "id".to_string(),
                to_column: "user_id".to_string(),
                cardinality: Cardinality::OneToMany,
            })
    }

    #[test]
    fn test_to_svg_contains_table_nodes() {
        let design = sample_design_with_relation();
        let editor = ErDiagramEditor::new(design);
        let svg = editor.to_svg();

        assert!(svg.contains("users"));
        assert!(svg.contains("orders"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<text"));
    }

    #[test]
    fn test_to_svg_contains_relation_edge() {
        let design = sample_design_with_relation();
        let editor = ErDiagramEditor::new(design);
        let svg = editor.to_svg();

        assert!(svg.contains("<line"));
        assert!(svg.contains("1:N"));
    }

    #[test]
    fn test_to_json_structure() {
        let design = sample_design_with_relation();
        let editor = ErDiagramEditor::new(design);
        let json = editor.to_json();

        let nodes = json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);

        let edges = json["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["from"], "users");
        assert_eq!(edges[0]["to"], "orders");
        assert_eq!(edges[0]["cardinality"], "1:N");
    }

    #[test]
    fn test_empty_design_svg() {
        let design = SchemaDesign::new(Dialect::MySql);
        let editor = ErDiagramEditor::new(design);
        let svg = editor.to_svg();
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_force_directed_layout() {
        let design = sample_design_with_relation();
        let mut editor = ErDiagramEditor::new(design);
        editor.layout(LayoutAlgorithm::ForceDirected);
        let svg = editor.to_svg();
        assert!(svg.contains("<svg"));
    }
}
