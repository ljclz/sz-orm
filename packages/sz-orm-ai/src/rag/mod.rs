use crate::embedding::EmbeddingModel;
use crate::error::AiError;
use crate::vector::VectorStore;

pub struct RagConfig {
    pub collection_name: String,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub top_k: usize,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            collection_name: "rag_documents".to_string(),
            chunk_size: 512,
            chunk_overlap: 50,
            top_k: 3,
        }
    }
}

impl RagConfig {
    pub fn new(collection_name: impl Into<String>) -> Self {
        Self {
            collection_name: collection_name.into(),
            ..Default::default()
        }
    }

    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    pub fn with_chunk_overlap(mut self, overlap: usize) -> Self {
        self.chunk_overlap = overlap;
        self
    }

    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub content: String,
    pub source: Option<String>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Document {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            source: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub index: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Chunk {
    pub fn new(
        id: impl Into<String>,
        document_id: impl Into<String>,
        content: impl Into<String>,
        index: usize,
        start_char: usize,
        end_char: usize,
    ) -> Self {
        Self {
            id: id.into(),
            document_id: document_id.into(),
            content: content.into(),
            index,
            start_char,
            end_char,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

pub struct RagEngine<E, V>
where
    E: EmbeddingModel,
    V: VectorStore,
{
    embedding_model: E,
    vector_store: V,
    config: RagConfig,
}

impl<E, V> RagEngine<E, V>
where
    E: EmbeddingModel,
    V: VectorStore,
{
    pub fn new(embedding_model: E, vector_store: V, config: RagConfig) -> Self {
        Self {
            embedding_model,
            vector_store,
            config,
        }
    }

    pub fn with_config(mut self, config: RagConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn index_documents(&self, documents: Vec<Document>) -> Result<usize, AiError> {
        let collection = &self.config.collection_name;
        let dimension = self.embedding_model.dimension();

        self.vector_store
            .create_collection(collection, dimension, None)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        let chunks = self.split_documents(documents);
        let mut total_indexed = 0;

        for chunk in chunks {
            let vector = self.embedding_model.embed(&chunk.content).await?;

            let record = crate::vector::VectorRecord::new(chunk.id.clone(), vector)
                .with_metadata(chunk.metadata);

            self.vector_store
                .insert(collection, vec![record])
                .await
                .map_err(|e| AiError::Vector(e.to_string()))?;

            total_indexed += 1;
        }

        Ok(total_indexed)
    }

    fn split_documents(&self, documents: Vec<Document>) -> Vec<Chunk> {
        let mut chunks = Vec::new();

        for document in documents {
            let content = document.content.as_str();
            let chars: Vec<char> = content.chars().collect();
            let chunk_size = self.config.chunk_size;
            let overlap = self.config.chunk_overlap;

            let mut start = 0;
            let mut index = 0;

            while start < chars.len() {
                let end = (start + chunk_size).min(chars.len());
                let chunk_content: String = chars[start..end].iter().collect();

                if !chunk_content.trim().is_empty() {
                    let chunk = Chunk::new(
                        format!("{}_{}", document.id, index),
                        document.id.clone(),
                        chunk_content,
                        index,
                        start,
                        end,
                    )
                    .with_metadata("source", document.source.clone().unwrap_or_default().into());

                    chunks.push(chunk);
                }

                if end == chars.len() {
                    break;
                }

                start = end - overlap.min(end);
                index += 1;
            }
        }

        chunks
    }

    pub async fn search(
        &self,
        query: &str,
        filter: Option<&str>,
    ) -> Result<Vec<RagSearchResult>, AiError> {
        let query_vector = self.embedding_model.embed(query).await?;

        let results = self
            .vector_store
            .search(
                &self.config.collection_name,
                &query_vector,
                self.config.top_k,
                filter,
            )
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|r| RagSearchResult {
                id: r.id,
                score: r.score,
                content: r.text.unwrap_or_default(),
                metadata: r.metadata.unwrap_or_default(),
            })
            .collect())
    }

    pub async fn delete_document(&self, document_id: &str) -> Result<u64, AiError> {
        let count = self
            .vector_store
            .count(&self.config.collection_name)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        if count == 0 {
            return Ok(0);
        }

        let dimension = self.embedding_model.dimension();
        let dummy_vector = vec![0.0; dimension];

        let all_results = self
            .vector_store
            .search(&self.config.collection_name, &dummy_vector, count, None)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        let to_delete: Vec<String> = all_results
            .into_iter()
            .filter(|r| r.id.starts_with(document_id))
            .map(|r| r.id)
            .collect();

        if to_delete.is_empty() {
            return Ok(0);
        }

        self.vector_store
            .delete(&self.config.collection_name, to_delete)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct RagSearchResult {
    pub id: String,
    pub score: f32,
    pub content: String,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}
