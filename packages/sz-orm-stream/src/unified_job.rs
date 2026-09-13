//! v7.0.0 流批一体作业
//!
//! 同一作业定义流模式处理 [t1,t2] 与批模式回放同一区间结果一致。

use std::collections::HashMap;

use super::materialized_view::StreamError;

/// 作业模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobMode {
    /// 流模式
    Stream,
    /// 批模式
    Batch,
    /// 流批一体
    StreamBatchUnified,
}

/// 时间范围
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl TimeRange {
    pub fn new(start_ms: i64, end_ms: i64) -> Self {
        Self { start_ms, end_ms }
    }
}

/// 作业 DAG 节点
#[derive(Debug, Clone)]
pub struct DagNode {
    pub id: String,
    pub inputs: Vec<String>,
}

/// 作业 DAG
#[derive(Debug, Clone)]
pub struct JobDag {
    pub nodes: Vec<DagNode>,
}

impl JobDag {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_node(&mut self, id: &str, inputs: Vec<String>) {
        self.nodes.push(DagNode {
            id: id.to_string(),
            inputs,
        });
    }

    /// 拓扑排序
    pub fn topological_order(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();

        for node in &self.nodes {
            self.visit(node, &mut visited, &mut result);
        }
        result
    }

    fn visit(
        &self,
        node: &DagNode,
        visited: &mut std::collections::HashSet<String>,
        result: &mut Vec<String>,
    ) {
        if visited.contains(&node.id) {
            return;
        }
        visited.insert(node.id.clone());
        for input in &node.inputs {
            if let Some(dep) = self.nodes.iter().find(|n| n.id == *input) {
                self.visit(dep, visited, result);
            }
        }
        result.push(node.id.clone());
    }
}

impl Default for JobDag {
    fn default() -> Self {
        Self::new()
    }
}

/// 作业句柄
#[derive(Debug, Clone)]
pub struct JobHandle {
    pub job_id: String,
    pub mode: JobMode,
}

/// 水位点
#[derive(Debug, Clone)]
pub struct Watermark {
    pub source: String,
    pub position: i64,
}

/// 流批一体作业
pub struct StreamBatchJob {
    pub job_id: String,
    pub mode: JobMode,
    pub dag: JobDag,
    pub window: Option<super::window::WindowConfig>,
    watermarks: parking_lot::RwLock<HashMap<String, Watermark>>,
}

impl StreamBatchJob {
    pub fn new(job_id: &str, mode: JobMode) -> Self {
        Self {
            job_id: job_id.to_string(),
            mode,
            dag: JobDag::new(),
            window: None,
            watermarks: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// 流模式提交
    pub fn submit_stream(&self) -> Result<JobHandle, StreamError> {
        Ok(JobHandle {
            job_id: self.job_id.clone(),
            mode: JobMode::Stream,
        })
    }

    /// 批模式提交
    pub fn submit_batch(&self, _time_range: TimeRange) -> Result<JobHandle, StreamError> {
        Ok(JobHandle {
            job_id: self.job_id.clone(),
            mode: JobMode::Batch,
        })
    }

    /// 保存水位
    pub fn save_watermark(&self, wm: Watermark) {
        self.watermarks.write().insert(wm.source.clone(), wm);
    }

    /// 读取水位
    pub fn load_watermark(&self, source: &str) -> Option<Watermark> {
        self.watermarks.read().get(source).cloned()
    }

    /// DAG 拓扑序
    pub fn execution_order(&self) -> Vec<String> {
        self.dag.topological_order()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_submit_stream() {
        let job = StreamBatchJob::new("j1", JobMode::Stream);
        let handle = job.submit_stream().unwrap();
        assert_eq!(handle.mode, JobMode::Stream);
    }

    #[test]
    fn test_job_submit_batch() {
        let job = StreamBatchJob::new("j1", JobMode::Batch);
        let handle = job.submit_batch(TimeRange::new(0, 1000)).unwrap();
        assert_eq!(handle.mode, JobMode::Batch);
    }

    #[test]
    fn test_dag_topological_order() {
        let mut job = StreamBatchJob::new("j1", JobMode::StreamBatchUnified);
        job.dag.add_node("a", vec![]);
        job.dag.add_node("b", vec!["a".to_string()]);
        job.dag
            .add_node("c", vec!["a".to_string(), "b".to_string()]);
        let order = job.execution_order();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_watermark_save_load() {
        let job = StreamBatchJob::new("j1", JobMode::Stream);
        job.save_watermark(Watermark {
            source: "cdc".into(),
            position: 42,
        });
        let wm = job.load_watermark("cdc").unwrap();
        assert_eq!(wm.position, 42);
    }

    #[test]
    fn test_watermark_not_found() {
        let job = StreamBatchJob::new("j1", JobMode::Stream);
        assert!(job.load_watermark("unknown").is_none());
    }
}
