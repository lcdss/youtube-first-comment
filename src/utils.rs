pub fn format_duration(seconds: u64) -> String {
  let hours = seconds / 3600;
  let minutes = (seconds % 3600) / 60;
  let seconds = seconds % 60;

  let mut parts = Vec::new();

  if hours > 0 {
    parts.push(format!("{}h", hours));
  }

  if minutes > 0 {
    parts.push(format!("{}m", minutes));
  }

  if seconds > 0 {
    parts.push(format!("{}s", seconds));
  }

  parts.join(" ")
}
