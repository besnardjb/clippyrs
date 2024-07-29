
**ClippyRS: A Rust CLI Client for OLLAMA**

ClippyRS is a command-line interface (CLI) client built using Rust, designed to interact with the [ollama](https://ollama.com). The idea is to provide a compact interface to communicate with ollama for repetitive tasks.

**Features**

* Parse the `OLLAMA_HOST` environment variable to determine the base URL of the OLLAMA API
* Tries localhost on default port otherwise
* If your input starts with '!' result will be displayed in a markdown-aware pager

**Getting Started**

1. **Install Rust**: Make sure you have Rust installed on your system. You can download it from [rustup.rs](https://rustup.rs).
2. **Clone the repository**: Run `git clone https://github.com/besnardjb/clippyrs.git` to get a copy of the ClippyRS codebase.
3. **Build and run**: Navigate into the cloned directory, then execute the following commands:
```bash
cargo build --release
# If needed
export OLLAMA_HOST="https://your-ollama-instance.com"
./target/release/clippyrs
```
This will compile ClippyRS with optimizations enabled, you may optionnaly set the `OLLAMA_HOST` environment variable to point to your OLLAMA instance, and run the application.
