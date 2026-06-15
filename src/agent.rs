use crate::{
    error::Result,
    providers::{Provider, ProviderRequest},
};

pub trait Agent {
    fn respond(&mut self, prompt: &str) -> Result<String>;

    fn respond_stream(
        &mut self,
        prompt: &str,
        mut on_chunk: impl FnMut(&str) -> Result<()>,
    ) -> Result<String> {
        let response = self.respond(prompt)?;
        if !response.is_empty() {
            on_chunk(&response)?;
        }

        Ok(response)
    }
}

pub struct ProviderAgent<P>
where
    P: Provider,
{
    provider: P,
}

impl<P> ProviderAgent<P>
where
    P: Provider,
{
    pub const fn new(provider: P) -> Self {
        Self { provider }
    }
}

impl<P> Agent for ProviderAgent<P>
where
    P: Provider,
{
    fn respond(&mut self, prompt: &str) -> Result<String> {
        self.respond_stream(prompt, |_| Ok(()))
    }

    fn respond_stream(
        &mut self,
        prompt: &str,
        on_chunk: impl FnMut(&str) -> Result<()>,
    ) -> Result<String> {
        let request = ProviderRequest {
            prompt: prompt.to_owned(),
            cwd: std::env::current_dir()?,
        };
        self.provider
            .complete_stream(request, on_chunk)
            .map(|response| response.text)
    }
}

#[derive(Debug, Default)]
pub struct EchoAgent;

impl Agent for EchoAgent {
    fn respond(&mut self, prompt: &str) -> Result<String> {
        Ok(format!("agent: {prompt}"))
    }
}
