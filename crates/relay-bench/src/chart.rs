//! Minimal SVG bar chart. No charting crate dependency.

pub struct Bar {
    pub label: String,
    pub value: f64,
    pub skipped: bool,
}

pub fn bar_chart(title: &str, bars: &[Bar]) -> String {
    let width = 720.0_f64;
    let height = 320.0_f64;
    let margin_left = 160.0_f64;
    let margin_right = 40.0_f64;
    let margin_top = 48.0_f64;
    let margin_bottom = 40.0_f64;
    let plot_w = width - margin_left - margin_right;
    let plot_h = height - margin_top - margin_bottom;
    let n = bars.len().max(1) as f64;
    let bar_h = plot_h / n;
    let max_v = bars
        .iter()
        .filter(|b| !b.skipped)
        .map(|b| b.value)
        .fold(0.0_f64, f64::max)
        .max(1e-9);

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n"
    ));
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#0f1419\"/>\n");
    out.push_str(&format!(
        "<text x=\"24\" y=\"28\" fill=\"#e7ecf1\" font-family=\"ui-sans-serif,system-ui,sans-serif\" font-size=\"16\">{}</text>\n",
        escape(title)
    ));

    for (i, bar) in bars.iter().enumerate() {
        let y = margin_top + (i as f64) * bar_h + bar_h * 0.15;
        let h = bar_h * 0.7;
        let w = if bar.skipped {
            plot_w * 0.08
        } else {
            plot_w * (bar.value / max_v)
        };
        let fill = if bar.skipped {
            "#4a5560"
        } else if bar.label.starts_with("relay") {
            "#3dd68c"
        } else {
            "#5b8def"
        };
        out.push_str(&format!(
            "<text x=\"{}\" y=\"{:.1}\" fill=\"#c5ced6\" font-family=\"ui-sans-serif,system-ui,sans-serif\" font-size=\"12\" text-anchor=\"end\">{}</text>\n",
            margin_left - 8.0,
            y + h * 0.7,
            escape(&bar.label),
        ));
        out.push_str(&format!(
            "<rect x=\"{margin_left}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{fill}\" rx=\"3\"/>\n"
        ));
        let caption = if bar.skipped {
            "n/a".to_string()
        } else {
            format!("{:.3}", bar.value)
        };
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#e7ecf1\" font-family=\"ui-sans-serif,system-ui,sans-serif\" font-size=\"11\">{caption}</text>\n",
            margin_left + w + 8.0,
            y + h * 0.7,
        ));
    }

    out.push_str("</svg>\n");
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
