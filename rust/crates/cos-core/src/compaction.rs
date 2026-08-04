use crate::models::{ChatMessage, MessageRole};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionResult {
    pub prompt_context: String,
    pub compacted_summary: Option<String>,
    pub estimated_tokens: usize,
    pub did_compact: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CompactionEngine;

impl CompactionEngine {
    pub fn estimate_tokens(text: &str) -> usize {
        ((text.len() as f64) / 3.8).ceil().max(1.0) as usize
    }

    pub fn prepare(
        messages: &[ChatMessage],
        previous_summary: Option<&str>,
        context_window: i64,
        threshold_percent: f64,
        keep_recent_tokens: i64,
    ) -> CompactionResult {
        let rendered = render(messages);
        let total = Self::estimate_tokens(&rendered);
        let threshold = (context_window as f64 * threshold_percent / 100.0) as usize;
        if total <= threshold || messages.len() <= 4 {
            let prefix = previous_summary
                .map(|summary| format!("Earlier context (compacted):\n{summary}\n\n"))
                .unwrap_or_default();
            return CompactionResult {
                prompt_context: format!("{prefix}{rendered}"),
                compacted_summary: previous_summary.map(str::to_string),
                estimated_tokens: total,
                did_compact: false,
            };
        }

        let mut recent: Vec<ChatMessage> = Vec::new();
        let mut used = 0usize;
        let keep = keep_recent_tokens.max(0) as usize;
        for message in messages.iter().rev() {
            let cost = Self::estimate_tokens(&message.content) + 8;
            if !recent.is_empty() && used + cost > keep {
                break;
            }
            recent.push(message.clone());
            used += cost;
        }
        recent.reverse();
        let old_count = messages.len().saturating_sub(recent.len());
        let older = &messages[..old_count];
        let summary = summarize(older, previous_summary);
        let prompt = format!(
            "Earlier context (compacted checkpoint):\n{summary}\n\nRecent verbatim context:\n{}",
            render(&recent)
        );
        CompactionResult {
            estimated_tokens: Self::estimate_tokens(&prompt),
            prompt_context: prompt,
            compacted_summary: Some(summary),
            did_compact: true,
        }
    }
}

fn render(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|message| format!("[{}]\n{}", message.role.raw_value().to_uppercase(), message.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn summarize(messages: &[ChatMessage], previous: Option<&str>) -> String {
    let mut lines: Vec<String> = Vec::new();
    if let Some(previous) = previous.filter(|value| !value.is_empty()) {
        lines.push(previous.to_string());
    }
    for message in messages {
        let clean = message.content.replace('\n', " ");
        let clipped = if clean.chars().count() > 420 {
            format!("{}…", clean.chars().take(420).collect::<String>())
        } else {
            clean
        };
        let role = match message.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        lines.push(format!("• {role}: {clipped}"));
    }
    let joined = lines.join("\n");
    if joined.chars().count() > 16_000 {
        let skip = joined.chars().count() - 16_000;
        joined.chars().skip(skip).collect()
    } else {
        joined
    }
}
