//! OpenAI API client and eye integration
//!
//! # Example
//! ```
//! use eye::providers::openai;
//!
//! let client = openai::Client::new("YOUR_API_KEY");
//!
//! let gpt4o = client.completion_model(openai::GPT_4O);
//! ```
use crate::{
    agent::AgentBuilder,
    completion::{self, CompletionError, CompletionRequest},
    embeddings::{self, EmbeddingError, EmbeddingsBuilder},
    extractor::ExtractorBuilder,
    json_utils, Embed,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

// ================================================================
// Main OpenAI Client
// ================================================================
const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Clone)]
pub struct Client {
    base_url: String,
    http_client: reqwest::Client,
}

impl Client {
    /// Create a new OpenAI client with the given API key.
    pub fn new(api_key: &str) -> Self {
        Self::from_url(api_key, OPENAI_API_BASE_URL)
    }

    /// Create a new OpenAI client with the given API key and base API URL.
    pub fn from_url(api_key: &str, base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            http_client: reqwest::Client::builder()
                .default_headers({
                    let mut headers = reqwest::header::HeaderMap::new();
                    headers.insert(
                        "Authorization",
                        format!("Bearer {}", api_key)
                            .parse()
                            .expect("Bearer token should parse"),
                    );
                    headers
                })
                .build()
                .expect("OpenAI reqwest client should build"),
        }
    }

    /// Create a new OpenAI client from the `OPENAI_API_KEY` environment variable.
    /// Panics if the environment variable is not set.
    pub fn from_env() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
        Self::new(&api_key)
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/{}", self.base_url, path).replace("//", "/");
        self.http_client.post(url)
    }

    /// Create an embedding model with the given name.
    /// Note: default embedding dimension of 0 will be used if model is not known.
    /// If this is the case, it's better to use function `embedding_model_with_ndims`
    ///
    /// # Example
    /// ```
    /// use eye::providers::openai::{Client, self};
    ///
    /// // Initialize the OpenAI client
    /// let openai = Client::new("your-open-ai-api-key");
    ///
    /// let embedding_model = openai.embedding_model(openai::TEXT_EMBEDDING_3_LARGE);
    /// ```
    pub fn embedding_model(&self, model: &str) -> EmbeddingModel {
        let ndims = match model {
            TEXT_EMBEDDING_3_LARGE => 3072,
            TEXT_EMBEDDING_3_SMALL | TEXT_EMBEDDING_ADA_002 => 1536,
            _ => 0,
        };
        EmbeddingModel::new(self.clone(), model, ndims)
    }

    /// Create an embedding model with the given name and the number of dimensions in the embedding generated by the model.
    ///
    /// # Example
    /// ```
    /// use eye::providers::openai::{Client, self};
    ///
    /// // Initialize the OpenAI client
    /// let openai = Client::new("your-open-ai-api-key");
    ///
    /// let embedding_model = openai.embedding_model("model-unknown-to-eye", 3072);
    /// ```
    pub fn embedding_model_with_ndims(&self, model: &str, ndims: usize) -> EmbeddingModel {
        EmbeddingModel::new(self.clone(), model, ndims)
    }

    /// Create an embedding builder with the given embedding model.
    ///
    /// # Example
    /// ```
    /// use eye::providers::openai::{Client, self};
    ///
    /// // Initialize the OpenAI client
    /// let openai = Client::new("your-open-ai-api-key");
    ///
    /// let embeddings = openai.embeddings(openai::TEXT_EMBEDDING_3_LARGE)
    ///     .simple_document("doc0", "Hello, world!")
    ///     .simple_document("doc1", "Goodbye, world!")
    ///     .build()
    ///     .await
    ///     .expect("Failed to embed documents");
    /// ```
    pub fn embeddings<D: Embed>(&self, model: &str) -> EmbeddingsBuilder<EmbeddingModel, D> {
        EmbeddingsBuilder::new(self.embedding_model(model))
    }

    /// Create a completion model with the given name.
    ///
    /// # Example
    /// ```
    /// use eye::providers::openai::{Client, self};
    ///
    /// // Initialize the OpenAI client
    /// let openai = Client::new("your-open-ai-api-key");
    ///
    /// let gpt4 = openai.completion_model(openai::GPT_4);
    /// ```
    pub fn completion_model(&self, model: &str) -> CompletionModel {
        CompletionModel::new(self.clone(), model)
    }

    /// Create an agent builder with the given completion model.
    ///
    /// # Example
    /// ```
    /// use eye::providers::openai::{Client, self};
    ///
    /// // Initialize the OpenAI client
    /// let openai = Client::new("your-open-ai-api-key");
    ///
    /// let agent = openai.agent(openai::GPT_4)
    ///    .preamble("You are comedian AI with a mission to make people laugh.")
    ///    .temperature(0.0)
    ///    .build();
    /// ```
    pub fn agent(&self, model: &str) -> AgentBuilder<CompletionModel> {
        AgentBuilder::new(self.completion_model(model))
    }

    /// Create an extractor builder with the given completion model.
    pub fn extractor<T: JsonSchema + for<'a> Deserialize<'a> + Serialize + Send + Sync>(
        &self,
        model: &str,
    ) -> ExtractorBuilder<T, CompletionModel> {
        ExtractorBuilder::new(self.completion_model(model))
    }
}

#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApiResponse<T> {
    Ok(T),
    Err(ApiErrorResponse),
}

// ================================================================
// OpenAI Embedding API
// ================================================================
/// `text-embedding-3-large` embedding model
pub const TEXT_EMBEDDING_3_LARGE: &str = "text-embedding-3-large";
/// `text-embedding-3-small` embedding model
pub const TEXT_EMBEDDING_3_SMALL: &str = "text-embedding-3-small";
/// `text-embedding-ada-002` embedding model
pub const TEXT_EMBEDDING_ADA_002: &str = "text-embedding-ada-002";

#[derive(Debug, Deserialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: Usage,
}

impl From<ApiErrorResponse> for EmbeddingError {
    fn from(err: ApiErrorResponse) -> Self {
        EmbeddingError::ProviderError(err.message)
    }
}

impl From<ApiResponse<EmbeddingResponse>> for Result<EmbeddingResponse, EmbeddingError> {
    fn from(value: ApiResponse<EmbeddingResponse>) -> Self {
        match value {
            ApiResponse::Ok(response) => Ok(response),
            ApiResponse::Err(err) => Err(EmbeddingError::ProviderError(err.message)),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f64>,
    pub index: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub total_tokens: usize,
}

impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Prompt tokens: {} Total tokens: {}",
            self.prompt_tokens, self.total_tokens
        )
    }
}

#[derive(Clone)]
pub struct EmbeddingModel {
    client: Client,
    pub model: String,
    ndims: usize,
}

impl embeddings::EmbeddingModel for EmbeddingModel {
    const MAX_DOCUMENTS: usize = 1024;

    fn ndims(&self) -> usize {
        self.ndims
    }

    #[cfg_attr(feature = "worker", worker::send)]
    async fn embed_texts(
        &self,
        documents: impl IntoIterator<Item = String>,
    ) -> Result<Vec<embeddings::Embedding>, EmbeddingError> {
        let documents = documents.into_iter().collect::<Vec<_>>();

        let response = self
            .client
            .post("/embeddings")
            .json(&json!({
                "model": self.model,
                "input": documents,
            }))
            .send()
            .await?;

        if response.status().is_success() {
            match response.json::<ApiResponse<EmbeddingResponse>>().await? {
                ApiResponse::Ok(response) => {
                    tracing::info!(target: "eye",
                        "OpenAI embedding token usage: {}",
                        response.usage
                    );

                    if response.data.len() != documents.len() {
                        return Err(EmbeddingError::ResponseError(
                            "Response data length does not match input length".into(),
                        ));
                    }

                    Ok(response
                        .data
                        .into_iter()
                        .zip(documents.into_iter())
                        .map(|(embedding, document)| embeddings::Embedding {
                            document,
                            vec: embedding.embedding,
                        })
                        .collect())
                }
                ApiResponse::Err(err) => Err(EmbeddingError::ProviderError(err.message)),
            }
        } else {
            Err(EmbeddingError::ProviderError(response.text().await?))
        }
    }
}

impl EmbeddingModel {
    pub fn new(client: Client, model: &str, ndims: usize) -> Self {
        Self {
            client,
            model: model.to_string(),
            ndims,
        }
    }
}

// ================================================================
// OpenAI Completion API
// ================================================================
/// `o1-preview` completion model
pub const O1_PREVIEW: &str = "o1-preview";
/// `o1-preview-2024-09-12` completion model
pub const O1_PREVIEW_2024_09_12: &str = "o1-preview-2024-09-12";
/// `o1-mini completion model
pub const O1_MINI: &str = "o1-mini";
/// `o1-mini-2024-09-12` completion model
pub const O1_MINI_2024_09_12: &str = "o1-mini-2024-09-12";
/// `gpt-4o` completion model
pub const GPT_4O: &str = "gpt-4o";
/// `gpt-4o-mini` completion model
pub const GPT_4O_MINI: &str = "gpt-4o-mini";
/// `gpt-4o-2024-05-13` completion model
pub const GPT_4O_2024_05_13: &str = "gpt-4o-2024-05-13";
/// `gpt-4-turbo` completion model
pub const GPT_4_TURBO: &str = "gpt-4-turbo";
/// `gpt-4-turbo-2024-04-09` completion model
pub const GPT_4_TURBO_2024_04_09: &str = "gpt-4-turbo-2024-04-09";
/// `gpt-4-turbo-preview` completion model
pub const GPT_4_TURBO_PREVIEW: &str = "gpt-4-turbo-preview";
/// `gpt-4-0125-preview` completion model
pub const GPT_4_0125_PREVIEW: &str = "gpt-4-0125-preview";
/// `gpt-4-1106-preview` completion model
pub const GPT_4_1106_PREVIEW: &str = "gpt-4-1106-preview";
/// `gpt-4-vision-preview` completion model
pub const GPT_4_VISION_PREVIEW: &str = "gpt-4-vision-preview";
/// `gpt-4-1106-vision-preview` completion model
pub const GPT_4_1106_VISION_PREVIEW: &str = "gpt-4-1106-vision-preview";
/// `gpt-4` completion model
pub const GPT_4: &str = "gpt-4";
/// `gpt-4-0613` completion model
pub const GPT_4_0613: &str = "gpt-4-0613";
/// `gpt-4-32k` completion model
pub const GPT_4_32K: &str = "gpt-4-32k";
/// `gpt-4-32k-0613` completion model
pub const GPT_4_32K_0613: &str = "gpt-4-32k-0613";
/// `gpt-3.5-turbo` completion model
pub const GPT_35_TURBO: &str = "gpt-3.5-turbo";
/// `gpt-3.5-turbo-0125` completion model
pub const GPT_35_TURBO_0125: &str = "gpt-3.5-turbo-0125";
/// `gpt-3.5-turbo-1106` completion model
pub const GPT_35_TURBO_1106: &str = "gpt-3.5-turbo-1106";
/// `gpt-3.5-turbo-instruct` completion model
pub const GPT_35_TURBO_INSTRUCT: &str = "gpt-3.5-turbo-instruct";

#[derive(Debug, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub system_fingerprint: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

impl From<ApiErrorResponse> for CompletionError {
    fn from(err: ApiErrorResponse) -> Self {
        CompletionError::ProviderError(err.message)
    }
}

impl TryFrom<CompletionResponse> for completion::CompletionResponse<CompletionResponse> {
    type Error = CompletionError;

    fn try_from(value: CompletionResponse) -> Result<Self, Self::Error> {
        match value.choices.as_slice() {
            [Choice {
                message:
                    Message {
                        tool_calls: Some(calls),
                        ..
                    },
                ..
            }, ..]
                if !calls.is_empty() =>
            {
                let call = calls.first().unwrap();

                Ok(completion::CompletionResponse {
                    choice: completion::ModelChoice::ToolCall(
                        call.function.name.clone(),
                        "".to_owned(),
                        serde_json::from_str(&call.function.arguments)?,
                    ),
                    raw_response: value,
                })
            }
            [Choice {
                message:
                    Message {
                        content: Some(content),
                        ..
                    },
                ..
            }, ..] => Ok(completion::CompletionResponse {
                choice: completion::ModelChoice::Message(content.to_string()),
                raw_response: value,
            }),
            _ => Err(CompletionError::ResponseError(
                "Response did not contain a message or tool call".into(),
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: Message,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: String,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: Function,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub r#type: String,
    pub function: completion::ToolDefinition,
}

impl From<completion::ToolDefinition> for ToolDefinition {
    fn from(tool: completion::ToolDefinition) -> Self {
        Self {
            r#type: "function".into(),
            function: tool,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Function {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone)]
pub struct CompletionModel {
    client: Client,
    /// Name of the model (e.g.: gpt-3.5-turbo-1106)
    pub model: String,
}

impl CompletionModel {
    pub fn new(client: Client, model: &str) -> Self {
        Self {
            client,
            model: model.to_string(),
        }
    }
}

impl completion::CompletionModel for CompletionModel {
    type Response = CompletionResponse;

    #[cfg_attr(feature = "worker", worker::send)]
    async fn completion(
        &self,
        mut completion_request: CompletionRequest,
    ) -> Result<completion::CompletionResponse<CompletionResponse>, CompletionError> {
        // Add preamble to chat history (if available)
        let mut full_history = if let Some(preamble) = &completion_request.preamble {
            vec![completion::Message {
                role: "system".into(),
                content: preamble.clone(),
            }]
        } else {
            vec![]
        };

        // Extend existing chat history
        full_history.append(&mut completion_request.chat_history);

        // Add context documents to chat history
        let prompt_with_context = completion_request.prompt_with_context();

        // Add context documents to chat history
        full_history.push(completion::Message {
            role: "user".into(),
            content: prompt_with_context,
        });

        let request = if completion_request.tools.is_empty() {
            json!({
                "model": self.model,
                "messages": full_history,
                "temperature": completion_request.temperature,
            })
        } else {
            json!({
                "model": self.model,
                "messages": full_history,
                "temperature": completion_request.temperature,
                "tools": completion_request.tools.into_iter().map(ToolDefinition::from).collect::<Vec<_>>(),
                "tool_choice": "auto",
            })
        };

        let response = self
            .client
            .post("/chat/completions")
            .json(
                &if let Some(params) = completion_request.additional_params {
                    json_utils::merge(request, params)
                } else {
                    request
                },
            )
            .send()
            .await?;

        if response.status().is_success() {
            match response.json::<ApiResponse<CompletionResponse>>().await? {
                ApiResponse::Ok(response) => {
                    tracing::info!(target: "eye",
                        "OpenAI completion token usage: {:?}",
                        response.usage.clone().map(|usage| format!("{usage}")).unwrap_or("N/A".to_string())
                    );
                    response.try_into()
                }
                ApiResponse::Err(err) => Err(CompletionError::ProviderError(err.message)),
            }
        } else {
            Err(CompletionError::ProviderError(response.text().await?))
        }
    }
}
