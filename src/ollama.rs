use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;
use std::{env, io};

use url::Url;
use url_open::UrlOpen;

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

/** Tool Call
*     "tool_calls": [
     {
       "function": {
         "name": "get_current_weather",
         "arguments": {
           "format": "celsius",
           "location": "Paris, FR"
         }
       }
     }
   ]
*/

#[derive(Serialize, Deserialize, Debug)]
struct ToolCall {
    name: String,
    parameters: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ToolCalls {
    function: Vec<ToolCall>,
}

/** Message
"message": {
    "role": "assistant",
    "content": "",
    "tool_calls": [
      {
        "function": {
          "name": "get_current_weather",
          "arguments": {
            "format": "celsius",
            "location": "Paris, FR"
          }
        }
      }
    ]
  }
*/

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    role: String,
    content: String,
    tool_calls: Option<ToolCalls>,
}

#[derive(Serialize, Debug)]
struct ToolFunctionParam {
    #[serde(rename = "type")]
    __type: String,
    description: String,
    #[serde(rename = "enum")]
    __enum: Option<Vec<String>>,
}

#[derive(Serialize, Debug)]
struct ToolFunctionParameters {
    #[serde(rename = "type")]
    __type: String,
    properties: HashMap<String, ToolFunctionParam>,
    required: Vec<String>,
}

#[derive(Serialize, Debug)]
struct ToolFunction {
    name: String,
    description: String,
    parameters: ToolFunctionParameters,
}

#[derive(Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    __type: String,
    function: ToolFunction,
    #[serde(skip_serializing)]
    closure: Box<dyn Fn(Vec<String>) -> String>,
}

impl Tool {
    /// An expression calculator
    pub fn calculator() -> Tool {
        let f = Box::new(|args: Vec<String>| {
            /* We need exactly two arguments */
            if args.len() != 1 {
                return "Operation failed as a single operand is needed".to_string();
            }

            match meval::eval_str(args.first().unwrap()) {
                Ok(resp) => format!("{}", resp),
                Err(e) => e.to_string(),
            }
        });

        let mut ret = Tool::new(
            "math_calculator",
            "A function computing the result of arbitrary mathematical expressions ",
            f,
        );
        ret.push_arg("expression", "string", "Expression to evaluate", None);
        ret.set_required("expression").unwrap();

        ret
    }

    pub fn url_open() -> Tool {
        let f = Box::new(|args: Vec<String>| {
            /* We need exactly two arguments */
            if args.len() != 1 {
                return "Operation failed as a single URL argument is needed".to_string();
            }

            if let Ok(url) = Url::parse(args.first().unwrap()) {
                url.open();
                "URL successfully opened".to_string()
            } else {
                "Failed to parse URL".to_string()
            }
        });

        let mut ret = Tool::new("open_url", "Use this to open an URL for the User.", f);
        ret.push_arg(
            "url",
            "string",
            "URL to open as correct HTTP(s) address",
            None,
        );
        ret.set_required("url").unwrap();

        ret
    }

    pub fn new(name: &str, description: &str, f: Box<dyn Fn(Vec<String>) -> String>) -> Tool {
        Tool {
            __type: "object".to_string(),
            closure: f,
            function: ToolFunction {
                name: name.to_string(),
                description: description.to_string(),
                parameters: ToolFunctionParameters {
                    __type: "object".to_string(),
                    properties: HashMap::new(),
                    required: vec![],
                },
            },
        }
    }

    pub fn push_arg(
        &mut self,
        name: &str,
        atype: &str,
        description: &str,
        optenum: Option<Vec<String>>,
    ) {
        self.function.parameters.properties.insert(
            name.to_string(),
            ToolFunctionParam {
                __type: atype.to_string(),
                description: description.to_string(),
                __enum: optenum,
            },
        );
    }

    fn extract_args(&self, parameters: HashMap<String, String>) -> Result<Vec<String>> {
        let mut ret: Vec<String> = Vec::new();

        /* Check for required args */
        for arg in self.function.parameters.properties.keys() {
            if let Some(prop) = parameters.get(arg) {
                ret.push(prop.clone());
            } else {
                return Err(anyhow!(
                    "No such argument '{}' to function '{}'",
                    arg,
                    self.function.name
                ));
            }
        }

        /* Check for extra arg */
        for arg in parameters.keys() {
            if !self.function.parameters.properties.contains_key(arg) {
                return Err(anyhow!(
                    "Function '{}' does not take a '{}' argument",
                    self.function.name,
                    arg
                ));
            }
        }

        Ok(ret)
    }

    pub fn set_required(&mut self, arg: &str) -> Result<()> {
        for key in self.function.parameters.properties.keys() {
            if *key == arg {
                self.function.parameters.required.push(key.clone());
                return Ok(());
            }
        }

        Err(anyhow!(
            "No such parameter {} in {}",
            arg,
            self.function.name
        ))
    }

    pub fn register_defaults(chat: &mut Chat) {
        chat.add_tool(Tool::calculator());
        chat.add_tool(Tool::url_open());
    }
}

#[derive(Serialize)]
pub struct Chat {
    model: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
}

impl Chat {
    fn new(model: &str) -> Chat {
        Chat {
            model: model.to_string(),
            messages: vec![],
            tools: vec![],
        }
    }

    fn init(model: &str, prompt: &str) -> Chat {
        Chat {
            model: model.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
                tool_calls: None,
            }],
            tools: vec![],
        }
    }

    pub fn add_tool(&mut self, tool: Tool) {
        self.tools.push(tool);
    }

    pub fn get_tool(&self, name: &str) -> Option<Rc<&Tool>> {
        for t in self.tools.iter() {
            if t.function.name == name {
                return Some(Rc::new(t));
            }
        }

        None
    }

    fn add_prompt(&mut self, prompt: &str) {
        self.messages.push(Message {
            role: "user".to_string(),
            content: prompt.to_string(),
            tool_calls: None,
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

    pub fn print_models(&self) {
        if let Some(models) = &self.models {
            for m in models.iter() {
                println!(
                    "- {} {} {}",
                    m.name,
                    m.details.family,
                    m.details.parameter_size.clone().unwrap_or("".to_string())
                );
            }
        }
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
            } else if !ollamahost.starts_with("http") {
                (format!("http://{}", ollamahost), 11434)
            } else {
                return Err(anyhow!("Failed to parse OLLAMA_HOST {}", ollamahost));
            }
        } else {
            ("http://localhost".to_string(), 11434)
        };

        Ollama::init(&host, port).await
    }

    pub fn set_model(&mut self, model: &str) -> Result<()> {
        /* Make sure model exists */
        let mut tmp_model = model.to_string();

        if let Some(models) = &self.models {
            let model_list: Vec<String> = models.iter().map(|v| v.name.clone()).collect();

            if !model_list.contains(&tmp_model) {
                // Try to append :latest
                tmp_model += ":latest";
                if !model_list.contains(&tmp_model) {
                    return Err(anyhow!(
                        "Cannot load model '{}' available models are {:?}",
                        model,
                        model_list
                    ));
                }
            }
        }

        self.current_model = Some(tmp_model);

        Ok(())
    }

    pub fn context_new(&self) -> Result<Chat> {
        if let Some(model) = &self.current_model {
            Ok(Chat::new(model.as_str()))
        } else {
            Err(anyhow!("No current model set"))
        }
    }

    pub async fn chat(&self, prompt: Option<&str>, context: &mut Chat) -> Result<bool> {
        /* Add user request */
        if let Some(prompt) = prompt {
            context.add_prompt(prompt);
        }

        let client = reqwest::Client::new();

        let mut res = client
            .post(self.endpoint("api/chat"))
            .json(&context)
            .send()
            .await?
            .bytes_stream()
            .map(|x| x.unwrap());

        //One line here
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

        println!();

        /* Check if last command is a function call */
        let call = match serde_json::from_str::<ToolCall>(assistant_resp.as_str()) {
            Ok(call) => Some(call),
            Err(_) => None,
        };

        context.messages.push(Message {
            role: "assistant".to_string(),
            content: assistant_resp,
            tool_calls: None,
        });

        if let Some(call) = call {
            if let Some(tool) = context.get_tool(&call.name) {
                match tool.extract_args(call.parameters) {
                    Ok(args) => {
                        let resp = (tool.closure)(args);
                        context.messages.push(Message {
                            role: "tool".to_string(),
                            content: resp,
                            tool_calls: None,
                        });
                    }
                    Err(e) => {
                        context.messages.push(Message {
                            role: "tool".to_string(),
                            content: format!("Error calling {} : {}", call.name, e),
                            tool_calls: None,
                        });
                    }
                }

                return Ok(true);
            }
        }

        Ok(false)
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
