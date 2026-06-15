use crate::{
    error::Result,
    providers::{Provider, ProviderRequest},
};

pub trait Agent {
    fn respond(&mut self, prompt: &str) -> Result<String>;
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
        let request = ProviderRequest {
            prompt: prompt.to_owned(),
            cwd: std::env::current_dir()?,
        };
        self.provider
            .complete(request)
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
