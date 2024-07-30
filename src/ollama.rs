use anyhow::{anyhow, Result};
use futures::StreamExt;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{env, io};

/* Model Description */

/**
*     {
       "name": "mistral:latest",
       "model": "mistral:latest",
       "size": 5137025024,
       "digest": "2ae6f6dd7a3dd734790bbbf58b8909a606e0e7e97e94b7604e0aa7ae4490e6d8",
       "details": {
         "parent_model": "",
         "format": "gguf",
         "family": "llama",
         "families": [
           "llama"
         ],
         "parameter_size": "7.2B",
         "quantization_level": "Q4_0"
       },
       "expires_at": "2024-06-04T14:38:31.83753-07:00",
       "size_vram": 5137025024
     }


     {
      "name": "codellama:13b",
      "modified_at": "2023-11-04T14:56:49.277302595-07:00",
      "size": 7365960935,
      "digest": "9f438cb9cd581fc025612d27f7c1a6669ff83a8bb0ed86c94fcf4c5440555697",
      "details": {
        "format": "gguf",
        "family": "llama",
        "families": null,
        "parameter_size": "13B",
        "quantization_level": "Q4_0"
      }
*/
#[derive(Serialize, Deserialize, Debug)]
struct OllamaModelDetails {
    parent_model: String,
    format: String,
    family: String,
    families: Option<Vec<String>>,
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct OllamaModel {
    pub name: String,
    model: Option<String>,
    size: usize,
    digest: String,
    details: OllamaModelDetails,
    expires_at: Option<String>,
    size_vram: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
struct OllamaModels {
    models: Vec<OllamaModel>,
}

/** Message
 *
 * {
  "model": "llama3",
  "messages": [
    {
      "role": "user",
      "content": "why is the sky blue?"
    }
  ]
}
*/

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    role: String,
    content: String,
}

#[derive(Serialize, Debug)]
pub struct Chat {
    model: String,
    messages: Vec<Message>,
}

impl Chat {
    fn new(model: &str) -> Chat {
        Chat {
            model: model.to_string(),
            messages: vec![],
        }
    }

    fn init(model: &str, prompt: &str) -> Chat {
        Chat {
            model: model.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        }
    }

    fn add_prompt(&mut self, prompt: &str) {
        self.messages.push(Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        })
    }

    pub fn response(&self) -> Option<String> {
        self.messages
            .iter()
            .filter(|v| v.role == "assistant")
            .last()
            .map(|v| v.content.clone())
    }
}

/** Chat response
 * {
  "model": "registry.ollama.ai/library/llama3:latest",
  "created_at": "2023-12-12T14:13:43.416799Z",
  "message": {
    "role": "assistant",
    "content": "Hello! How are you today?"
  },
  "done": true,
  "total_duration": 5191566416,
  "load_duration": 2154458,
  "prompt_eval_count": 26,
  "prompt_eval_duration": 383809000,
  "eval_count": 298,
  "eval_duration": 4799921000
}
 */
#[derive(Deserialize, Debug)]
struct ChatResponse {
    model: String,
    created_at: String,
    message: Message,
    done: bool,
    total_duration: Option<u64>,
    load_duration: Option<u64>,
    prompt_eval_count: Option<u64>,
    prompt_eval_duration: Option<u64>,
    eval_count: Option<u64>,
    eval_duration: Option<u64>,
}

pub struct Ollama {
    host: String,
    port: i32,
    models: Option<Vec<OllamaModel>>,
    current_model: Option<String>,
}

impl Ollama {
    fn url(&self) -> String {
        if self.port != 0 {
            format!("{}:{}", self.host, self.port)
        } else {
            self.host.clone()
        }
    }

    fn endpoint(&self, ep: &str) -> String {
        let ret = format!("{}/{}", self.url(), ep).replace("//", "/");
        info!("Endpoint is {}", ret);
        ret
    }

    async fn loaded_models(&self) -> Result<OllamaModels> {
        let resp = reqwest::get(self.endpoint("api/ps")).await?;
        let models: OllamaModels = resp.json().await?;

        Ok(models)
    }

    async fn list_models(&self) -> Result<OllamaModels> {
        let resp = reqwest::get(self.endpoint("api/tags")).await?;
        let models: OllamaModels = resp.json().await?;

        Ok(models)
    }

    pub async fn default() -> Result<Ollama> {
        let (host, port) = if let Ok(ollamahost) = env::var("OLLAMA_HOST") {
            let addr_no_proto = ollamahost.replace("http://", "").replace("https://", "");

            if addr_no_proto.contains(':') {
                let sp: Vec<&str> = addr_no_proto.split(':').collect();

                if let (Some(host), Some(port)) = (sp.first(), sp.get(1)) {
                    println!("PORT : {}", port);
                    let port = port.parse::<i32>()?;
                    (host.to_string(), port)
                } else {
                    return Err(anyhow!("Failed to parse host and port from {}", ollamahost));
                }
            } else if ollamahost.starts_with("https://") || ollamahost.starts_with("http://") {
                (ollamahost, 0)
            } else {
                (ollamahost, 11434)
            }
        } else {
            ("http://localhost".to_string(), 11434)
        };

        Ollama::init(&host, port).await
    }

    pub fn set_model(&mut self, model: &str) -> Result<()> {
        /* Make sure model exists */
        if let Some(models) = &self.models {
            let model_list: Vec<String> = models.iter().map(|v| v.name.clone()).collect();

            if !model_list.contains(&model.to_string()) {
                return Err(anyhow!(
                    "Cannot load model '{}' available models are {:?}",
                    model,
                    model_list
                ));
            }
        }

        self.current_model = Some(model.to_string());

        Ok(())
    }

    pub fn context_new(&self) -> Result<Chat> {
        if let Some(model) = &self.current_model {
            Ok(Chat::new(model.as_str()))
        } else {
            Err(anyhow!("No current model set"))
        }
    }

    pub async fn chat(&self, prompt: &str, context: &mut Chat) -> Result<()> {
        /* Add user request */
        context.add_prompt(prompt);

        let client = reqwest::Client::new();

        let mut res = client
            .post(self.endpoint("api/chat"))
            .json(&context)
            .send()
            .await?
            .bytes_stream()
            .map(|x| x.unwrap());

        //One line here
        print!("\nAssistant:");

        let mut assistant_resp = String::new();

        while let Some(item) = res.next().await {
            let s = std::str::from_utf8(&item)?.trim();

            for line in s.split('\n') {
                //println!("'{}'", line);
                match serde_json::from_str::<ChatResponse>(line) {
                    Ok(chat_resp) => {
                        assistant_resp += chat_resp.message.content.as_str();
                        print!("{}", chat_resp.message.content);
                        io::stdout().flush()?;
                    }
                    Err(e) => {
                        error!("Failed to parse response '{}' : {}", line, e);
                    }
                }
            }
        }

        context.messages.push(Message {
            role: "assistant".to_string(),
            content: assistant_resp,
        });

        Ok(())
    }

    pub async fn init(host: &str, port: i32) -> Result<Ollama> {
        /* First check the server availaibility */

        info!("Connecting to {} : {}", host, port);

        let mut ret = Ollama {
            host: host.to_string(),
            port,
            models: None,
            current_model: None,
        };

        /* Here negotiate a model to use from current state
        it is also the opportunity to probe the API */

        ret.models = Some(ret.list_models().await?.models);
        let current_model = ret.loaded_models().await?;
        ret.current_model = if let Some(first) = current_model.models.first() {
            log::info!("Using loaded model '{}'", first.name);
            Some(first.name.clone())
        } else if let Some(avail_models) = &ret.models {
            let ret: Option<String> = avail_models.first().map(|first| first.name.clone());
            if let Some(m) = &ret {
                log::info!("Using first available model '{}'", m);
            }
            ret
        } else {
            None
        };

        if ret.current_model.is_none() {
            log::info!("Using default model 'llama3.1:latest'");
            /* Use a reasonable default (will certainly be overriden) */
            ret.current_model = Some("llama3.1:latest".to_string());
        }

        Ok(ret)
    }
}
