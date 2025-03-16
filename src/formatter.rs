/// Formats a markdown document according to basic markdown styling rules:
/// - Trims trailing whitespace from each line.
/// - Ensures that any heading (lines starting with '#' markers) has exactly one space after the '#' characters.
/// - For any heading line, inserts an empty line immediately after.
/// - Trims leading whitespace from non-heading lines.
/// - Collapses multiple blank lines into a single blank line.
/// - Ensures the output ends with a newline.
pub fn format_markdown(input: &str) -> String {
    let mut output_lines = Vec::new();
    let mut prev_blank = false;
    for line in input.lines() {
        // Remove trailing whitespace.
        let trimmed = line.trim_end();
        let formatted_line = if trimmed.starts_with('#') {
            // Count the '#' markers.
            let mut count = 0;
            for c in trimmed.chars() {
                if c == '#' {
                    count += 1;
                } else {
                    break;
                }
            }
            if trimmed.len() > count {
                let rest = &trimmed[count..];
                // Ensure exactly one space after the '#' markers.
                if !rest.starts_with(' ') {
                    format!("{} {}", "#".repeat(count), rest.trim_start())
                } else {
                    format!("{} {}", "#".repeat(count), rest.trim())
                }
            } else {
                "#".repeat(count)
            }
        } else {
            // For non-heading lines, trim any leading whitespace.
            trimmed.trim_start().to_string()
        };

        let is_blank = formatted_line.trim().is_empty();

        if !is_blank {
            // Push the formatted line.
            output_lines.push(formatted_line.clone());
            // If it's a heading, insert an extra blank line.
            if formatted_line.starts_with("#") {
                output_lines.push(String::new());
                prev_blank = true;
                continue;
            }
        } else {
            // Avoid multiple consecutive blank lines.
            if !prev_blank {
                output_lines.push(String::new());
            }
        }
        prev_blank = is_blank;
    }
    let mut result = output_lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_markdown() {
        let input = "\
#Heading1
Some text  
   More text

##Heading2
Text under heading2
";
        let expected = "\
# Heading1

Some text
More text

## Heading2

Text under heading2
";
        let output = format_markdown(input);
        assert_eq!(output, expected);
    }
}
