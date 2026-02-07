use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

pub struct AiClient {
    client: Client,
    api_key: String,
    api_url: String,
}

impl AiClient {
    pub fn new(api_key: String, api_url: String, proxy: Option<&str>) -> Self {
        let client = if let Some(proxy_url) = proxy {
            match reqwest::Proxy::all(proxy_url) {
                Ok(p) => Client::builder().proxy(p).build().unwrap_or_else(|_| Client::new()),
                Err(e) => {
                    log::warn!("代理配置无效 ({}): {}", proxy_url, e);
                    Client::new()
                }
            }
        } else {
            Client::new()
        };
        Self {
            client,
            api_key,
            api_url,
        }
    }

    pub async fn analyze_update(
        &self,
        prompt: &str,
        model: &str,
        temperature: f32,
    ) -> Result<String> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature,
            stream: false,
        };

        let response = self
            .client
            .post(&self.api_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            anyhow::bail!("API 请求失败 ({}): {}", status, error_text);
        }

        let chat_response: ChatResponse = response.json().await?;

        if let Some(choice) = chat_response.choices.first() {
            if let Some(usage) = chat_response.usage {
                log::info!(
                    "Token 使用: 输入={}, 输出={}, 总计={}",
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.total_tokens
                );
            }

            Ok(choice.message.content.clone())
        } else {
            anyhow::bail!("API 返回了空响应")
        }
    }
}
