use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const DEEPSEEK_API_URL: &str = "https://api.deepseek.com/chat/completions";

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
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

pub struct DeepSeekClient {
    client: Client,
    api_key: String,
}

impl DeepSeekClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
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
            .post(DEEPSEEK_API_URL)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "DeepSeek API 请求失败 (状态码 {}): {}",
                status,
                error_text
            ));
        }

        let chat_response: ChatResponse = response.json().await?;

        if let Some(choice) = chat_response.choices.first() {
            // 记录 token 使用情况
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
            Err(anyhow::anyhow!("DeepSeek API 返回了空响应"))
        }
    }
}
