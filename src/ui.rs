use std::io::{self, Write};

use crate::session::SessionResponse;

pub struct TerminalRenderer<W>
where
    W: Write,
{
    writer: W,
    agent_chunk_ends_with_newline: bool,
}

impl<W> TerminalRenderer<W>
where
    W: Write,
{
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            agent_chunk_ends_with_newline: true,
        }
    }

    pub fn render_prompt(&mut self, prompt: &str) -> io::Result<()> {
        write!(self.writer, "{prompt}")?;
        self.writer.flush()
    }

    pub fn begin_agent_response(&mut self, status: &str) -> io::Result<()> {
        self.agent_chunk_ends_with_newline = true;
        writeln!(self.writer)?;
        writeln!(self.writer, "assistant  {status}")?;
        writeln!(self.writer)?;
        self.writer.flush()
    }

    pub fn stream_agent_chunk(&mut self, chunk: &str) -> io::Result<()> {
        write!(self.writer, "{chunk}")?;
        self.agent_chunk_ends_with_newline = chunk.ends_with('\n');
        self.writer.flush()
    }

    pub fn end_agent_response(&mut self) -> io::Result<()> {
        if !self.agent_chunk_ends_with_newline {
            writeln!(self.writer)?;
        }
        writeln!(self.writer)?;
        self.writer.flush()
    }

    pub fn render_response(&mut self, response: &SessionResponse) -> io::Result<()> {
        match response {
            SessionResponse::Agent(text) => self.render_agent_response(text),
            SessionResponse::Command(result) => {
                let printed = !result.stdout.is_empty() || !result.stderr.is_empty();
                write!(self.writer, "{}", result.stdout)?;
                write!(self.writer, "{}", result.stderr)?;
                if printed {
                    if !result.stdout.ends_with('\n') && !result.stderr.ends_with('\n') {
                        writeln!(self.writer)?;
                    }
                    writeln!(self.writer)?;
                }
                self.writer.flush()
            }
            SessionResponse::ModeChanged(mode) => {
                writeln!(self.writer, "[ash] mode {}", mode.prompt())?;
                writeln!(self.writer)?;
                self.writer.flush()
            }
            SessionResponse::Empty => Ok(()),
        }
    }

    fn render_agent_response(&mut self, text: &str) -> io::Result<()> {
        self.begin_agent_response("")?;
        self.stream_agent_chunk(text)?;
        self.end_agent_response()
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalRenderer;

    #[test]
    fn agent_responses_have_padding_before_and_after_streamed_text() {
        let mut output = Vec::new();
        let mut renderer = TerminalRenderer::new(&mut output);

        renderer
            .begin_agent_response("[ash mode=> provider=codex cwd=/tmp/project]")
            .expect("begin");
        renderer.stream_agent_chunk("hello").expect("stream");
        renderer.end_agent_response().expect("end");

        let output = String::from_utf8(output).expect("utf8");
        assert_eq!(
            output,
            "\nassistant  [ash mode=> provider=codex cwd=/tmp/project]\n\nhello\n\n"
        );
    }

    #[test]
    fn prompt_is_rendered_without_extra_padding() {
        let mut output = Vec::new();
        let mut renderer = TerminalRenderer::new(&mut output);

        renderer.render_prompt("> ").expect("prompt");

        let output = String::from_utf8(output).expect("utf8");
        assert_eq!(output, "> ");
    }
}
