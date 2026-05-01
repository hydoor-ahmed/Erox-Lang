use crate::compiler::lexer::Span;

/// EROX Diagnostic Reporter — Pretty error output with source snippets.
pub struct DiagnosticReporter {
    source: String,
    filename: String,
}

impl DiagnosticReporter {
    pub fn new(source: &str, filename: &str) -> Self {
        DiagnosticReporter {
            source: source.to_string(),
            filename: filename.to_string(),
        }
    }

    fn get_line(&self, line: usize) -> Option<&str> {
        self.source.lines().nth(line.saturating_sub(1))
    }

    pub fn report_error(&self, span: Span, message: &str) -> String {
        self.format_diagnostic("Error", span, message, "\x1b[31m") // Red
    }

    pub fn report_warning(&self, span: Span, message: &str) -> String {
        self.format_diagnostic("Warning", span, message, "\x1b[33m") // Yellow
    }

    fn format_diagnostic(&self, level: &str, span: Span, message: &str, color: &str) -> String {
        let reset = "\x1b[0m";
        let dim = "\x1b[2m";
        let bold = "\x1b[1m";
        let cyan = "\x1b[36m";

        let mut output = String::new();

        // Header
        output.push_str(&format!(
            "\n{bold}{color}[{level}]{reset} {bold}→ {cyan}{file}:{line}:{col}{reset} {dim}|{reset} {message}\n",
            bold = bold, color = color, level = level, reset = reset,
            cyan = cyan, file = self.filename, line = span.line, col = span.col,
            dim = dim, message = message
        ));

        // Source snippet
        if let Some(line_text) = self.get_line(span.line) {
            let line_num = format!("{}", span.line);
            let padding = " ".repeat(line_num.len());

            // Context: line before
            if span.line > 1 {
                if let Some(prev_line) = self.get_line(span.line - 1) {
                    output.push_str(&format!(
                        "  {dim}{:>width$} │{reset} {}\n",
                        span.line - 1, prev_line, dim = dim, reset = reset, width = line_num.len()
                    ));
                }
            }

            // Error line
            output.push_str(&format!(
                "  {bold}{}{reset} {dim}│{reset} {}\n",
                line_num, line_text, bold = bold, reset = reset, dim = dim
            ));

            // Pointer
            let pointer_offset = if span.col > 0 { span.col - 1 } else { 0 };
            let pointer = format!("{}{}{color}^{reset}", " ".repeat(pointer_offset), "", color = color, reset = reset);
            output.push_str(&format!(
                "  {} {dim}│{reset} {}\n",
                padding, pointer, dim = dim, reset = reset
            ));

            // Context: line after
            if let Some(next_line) = self.get_line(span.line + 1) {
                output.push_str(&format!(
                    "  {dim}{:>width$} │{reset} {}\n",
                    span.line + 1, next_line, dim = dim, reset = reset, width = line_num.len()
                ));
            }
        }

        output
    }

    /// Format multiple errors with a final summary.
    pub fn report_errors(&self, errors: &[(Span, String)]) -> String {
        let mut output = String::new();
        for (span, msg) in errors {
            output.push_str(&self.report_error(*span, msg));
        }
        let reset = "\x1b[0m";
        let bold = "\x1b[1m";
        let red = "\x1b[31m";
        output.push_str(&format!(
            "\n{bold}{red}✗ {} error(s) found{reset}\n",
            errors.len(), bold = bold, red = red, reset = reset
        ));
        output
    }
}
