use std::io::{self, Write};

use crate::{
    session::SessionResponse,
    stream::{AgentStreamEvent, TokenUsage},
};

pub struct TerminalRenderer<W>
where
    W: Write,
{
    writer: W,
    agent_chunk_ends_with_newline: bool,
    agent_line_open: bool,
    current_section: Option<AgentSection>,
    style: TerminalStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalStyle {
    color: bool,
}

impl TerminalStyle {
    #[must_use]
    pub const fn color() -> Self {
        Self { color: true }
    }

    #[must_use]
    pub const fn plain() -> Self {
        Self { color: false }
    }

    const fn paint(self, role: StyleRole) -> &'static str {
        if !self.color {
            return "";
        }

        match role {
            StyleRole::Reset => "\x1b[0m",
            StyleRole::Panel => "\x1b[48;2;18;22;28m",
            StyleRole::Border => "\x1b[38;2;104;114;133m",
            StyleRole::Header => "\x1b[38;2;201;209;222m",
            StyleRole::Muted => "\x1b[38;2;137;148;168m",
            StyleRole::Body => "\x1b[38;2;231;236;244m",
            StyleRole::Accent => "\x1b[38;2;126;203;255m",
            StyleRole::Success => "\x1b[38;2;139;233;178m",
            StyleRole::Warning => "\x1b[38;2;255;213;128m",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum StyleRole {
    Reset,
    Panel,
    Border,
    Header,
    Muted,
    Body,
    Accent,
    Success,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentSection {
    Thinking,
    Tool,
    Output,
    Response,
    Usage,
}

impl AgentSection {
    const fn label(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Tool => "tool",
            Self::Output => "output",
            Self::Response => "response",
            Self::Usage => "usage",
        }
    }

    const fn role(self) -> StyleRole {
        match self {
            Self::Thinking | Self::Usage => StyleRole::Muted,
            Self::Tool => StyleRole::Accent,
            Self::Output => StyleRole::Warning,
            Self::Response => StyleRole::Header,
        }
    }
}

impl<W> TerminalRenderer<W>
where
    W: Write,
{
    pub const fn new(writer: W) -> Self {
        Self::with_style(writer, TerminalStyle::color())
    }

    pub const fn plain(writer: W) -> Self {
        Self::with_style(writer, TerminalStyle::plain())
    }

    pub const fn with_style(writer: W, style: TerminalStyle) -> Self {
        Self {
            writer,
            agent_chunk_ends_with_newline: true,
            agent_line_open: false,
            current_section: None,
            style,
        }
    }

    pub fn render_prompt(&mut self, prompt: &str) -> io::Result<()> {
        write!(self.writer, "{prompt}")?;
        self.writer.flush()
    }

    pub fn begin_agent_response(&mut self, status: &str) -> io::Result<()> {
        self.agent_chunk_ends_with_newline = true;
        self.agent_line_open = false;
        self.current_section = None;
        writeln!(self.writer)?;
        writeln!(
            self.writer,
            "{}{}╭─ {}assistant{} {}{}{}",
            self.style.paint(StyleRole::Panel),
            self.style.paint(StyleRole::Border),
            self.style.paint(StyleRole::Header),
            self.style.paint(StyleRole::Border),
            self.style.paint(StyleRole::Muted),
            status,
            self.style.paint(StyleRole::Reset),
        )?;
        self.write_agent_empty_line()?;
        self.writer.flush()
    }

    pub fn stream_agent_event(&mut self, event: &AgentStreamEvent) -> io::Result<()> {
        match event {
            AgentStreamEvent::Status(status) => {
                self.begin_section(AgentSection::Thinking)?;
                self.write_agent_lines(status, StyleRole::Muted)?;
            }
            AgentStreamEvent::ToolStarted { command } => {
                self.begin_section(AgentSection::Tool)?;
                self.write_agent_lines(&format!("$ {command}"), StyleRole::Accent)?;
            }
            AgentStreamEvent::ToolOutput(output) => {
                self.begin_section(AgentSection::Output)?;
                self.write_agent_lines(output, StyleRole::Body)?;
            }
            AgentStreamEvent::ToolCompleted { exit_code } => {
                self.begin_section(AgentSection::Tool)?;
                let status =
                    exit_code.map_or_else(|| "exit ?".to_owned(), |code| format!("exit {code}"));
                self.write_agent_lines(&status, StyleRole::Success)?;
            }
            AgentStreamEvent::AssistantText(text) => {
                self.begin_section(AgentSection::Response)?;
                self.write_agent_inline(text, StyleRole::Body)?;
            }
            AgentStreamEvent::Usage(usage) => {
                self.begin_section(AgentSection::Usage)?;
                self.write_agent_lines(&format_usage(*usage), StyleRole::Muted)?;
            }
        }
        self.writer.flush()
    }

    pub fn end_agent_response(&mut self) -> io::Result<()> {
        if self.agent_line_open || !self.agent_chunk_ends_with_newline {
            writeln!(self.writer)?;
        }
        writeln!(
            self.writer,
            "{}{}╰{}",
            self.style.paint(StyleRole::Panel),
            self.style.paint(StyleRole::Border),
            self.style.paint(StyleRole::Reset),
        )?;
        writeln!(self.writer)?;
        self.agent_line_open = false;
        self.current_section = None;
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
        self.stream_agent_event(&AgentStreamEvent::AssistantText(text.to_owned()))?;
        self.end_agent_response()
    }

    fn begin_section(&mut self, section: AgentSection) -> io::Result<()> {
        if self.current_section == Some(section) {
            return Ok(());
        }
        self.close_open_agent_line()?;
        if self.current_section.is_some() {
            self.write_agent_empty_line()?;
        }
        writeln!(
            self.writer,
            "{}{}│{} {}{}{}",
            self.style.paint(StyleRole::Panel),
            self.style.paint(StyleRole::Border),
            self.style.paint(StyleRole::Reset),
            self.style.paint(section.role()),
            section.label(),
            self.style.paint(StyleRole::Reset),
        )?;
        self.current_section = Some(section);
        Ok(())
    }

    fn write_agent_lines(&mut self, text: &str, role: StyleRole) -> io::Result<()> {
        for line in text.lines() {
            self.write_agent_prefix()?;
            writeln!(
                self.writer,
                "{}{}{}",
                self.style.paint(role),
                line,
                self.style.paint(StyleRole::Reset),
            )?;
        }

        if text.ends_with('\n') || text.is_empty() {
            return Ok(());
        }

        Ok(())
    }

    fn write_agent_inline(&mut self, text: &str, role: StyleRole) -> io::Result<()> {
        for segment in text.split_inclusive('\n') {
            if !self.agent_line_open {
                self.write_agent_prefix()?;
            }

            let content = segment.trim_end_matches('\n');
            write!(
                self.writer,
                "{}{}{}{}",
                self.style.paint(StyleRole::Panel),
                self.style.paint(role),
                content,
                self.style.paint(StyleRole::Reset),
            )?;

            if segment.ends_with('\n') {
                writeln!(self.writer)?;
                self.agent_line_open = false;
                self.agent_chunk_ends_with_newline = true;
            } else {
                self.agent_line_open = true;
                self.agent_chunk_ends_with_newline = false;
            }
        }
        Ok(())
    }

    fn close_open_agent_line(&mut self) -> io::Result<()> {
        if self.agent_line_open {
            writeln!(self.writer)?;
            self.agent_line_open = false;
            self.agent_chunk_ends_with_newline = true;
        }
        Ok(())
    }

    fn write_agent_empty_line(&mut self) -> io::Result<()> {
        writeln!(
            self.writer,
            "{}{}│{}",
            self.style.paint(StyleRole::Panel),
            self.style.paint(StyleRole::Border),
            self.style.paint(StyleRole::Reset),
        )
    }

    fn write_agent_prefix(&mut self) -> io::Result<()> {
        write!(
            self.writer,
            "{}{}│{}{} ",
            self.style.paint(StyleRole::Panel),
            self.style.paint(StyleRole::Border),
            self.style.paint(StyleRole::Reset),
            self.style.paint(StyleRole::Panel),
        )
    }
}

fn format_usage(usage: TokenUsage) -> String {
    let mut parts = vec![
        format!("in {}", usage.input_tokens),
        format!("out {}", usage.output_tokens),
    ];
    if let Some(cached) = usage.cached_input_tokens {
        parts.push(format!("cached {cached}"));
    }
    if let Some(reasoning) = usage.reasoning_output_tokens {
        parts.push(format!("reasoning {reasoning}"));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use crate::stream::{AgentStreamEvent, TokenUsage};

    use super::TerminalRenderer;

    #[test]
    fn agent_responses_have_padding_before_and_after_streamed_text() {
        let mut output = Vec::new();
        let mut renderer = TerminalRenderer::plain(&mut output);

        renderer
            .begin_agent_response("[ash mode=> provider=codex cwd=/tmp/project]")
            .expect("begin");
        renderer
            .stream_agent_event(&AgentStreamEvent::AssistantText("hello".to_owned()))
            .expect("stream");
        renderer.end_agent_response().expect("end");

        let output = String::from_utf8(output).expect("utf8");
        assert_eq!(
            output,
            "\n╭─ assistant [ash mode=> provider=codex cwd=/tmp/project]\n│\n│ response\n│ hello\n╰\n\n"
        );
    }

    #[test]
    fn prompt_is_rendered_without_extra_padding() {
        let mut output = Vec::new();
        let mut renderer = TerminalRenderer::plain(&mut output);

        renderer.render_prompt("> ").expect("prompt");

        let output = String::from_utf8(output).expect("utf8");
        assert_eq!(output, "> ");
    }

    #[test]
    fn agent_events_are_separated_into_minimal_sections() {
        let mut output = Vec::new();
        let mut renderer = TerminalRenderer::plain(&mut output);

        renderer.begin_agent_response("[ash]").expect("begin");
        renderer
            .stream_agent_event(&AgentStreamEvent::Status("started".to_owned()))
            .expect("status");
        renderer
            .stream_agent_event(&AgentStreamEvent::ToolStarted {
                command: "git status --short".to_owned(),
            })
            .expect("tool");
        renderer
            .stream_agent_event(&AgentStreamEvent::ToolOutput(" M src/ui.rs\n".to_owned()))
            .expect("output");
        renderer
            .stream_agent_event(&AgentStreamEvent::AssistantText("clean".to_owned()))
            .expect("response");
        renderer
            .stream_agent_event(&AgentStreamEvent::Usage(TokenUsage {
                input_tokens: 12,
                cached_input_tokens: Some(4),
                output_tokens: 3,
                reasoning_output_tokens: Some(1),
            }))
            .expect("usage");
        renderer.end_agent_response().expect("end");

        let output = String::from_utf8(output).expect("utf8");
        assert_eq!(
            output,
            "\n╭─ assistant [ash]\n│\n│ thinking\n│ started\n│\n│ tool\n│ $ git status --short\n│\n│ output\n│  M src/ui.rs\n│\n│ response\n│ clean\n│\n│ usage\n│ in 12 · out 3 · cached 4 · reasoning 1\n╰\n\n"
        );
    }
}
