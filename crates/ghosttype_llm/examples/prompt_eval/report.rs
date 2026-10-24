//! The report that the eval prints.

use std::io::{self, Write};

use crate::score::{Comparison, ReferenceScore, SizeTime, Summary};

pub fn write_report(
    output: &mut impl Write,
    summary: &Summary,
    comparison: Option<&Comparison>,
) -> io::Result<()> {
    writeln!(output, "next word:     {}", summary.next_word)?;
    writeln!(output, "partial word:  {}", summary.partial_word)?;
    if summary.typo.cases > 0 {
        writeln!(output, "typo word:     {}", summary.typo)?;
        writeln!(
            output,
            "typo fixes:    {} right, {} wrong",
            summary.typo_fixes.right, summary.typo_fixes.wrong
        )?;
    }
    writeln!(
        output,
        "other fixes:   {} right, {} wrong",
        summary.other_fixes.right, summary.other_fixes.wrong
    )?;
    if summary.no_new_line.cases > 0 {
        writeln!(output, "no new line:   {}", summary.no_new_line)?;
    }
    writeln!(output, "no suggestion: {}", summary.without_suggestion)?;
    writeln!(output, "median time:   {} ms", summary.median_millis)?;
    if let Some(millis) = summary.median_warm_millis {
        writeln!(output, "warm time:     {millis} ms")?;
    }
    if let Some(tokens) = summary.median_prompt_tokens {
        writeln!(output, "prompt tokens: {tokens}")?;
    }
    if let Some(millis) = summary.millis_per_token {
        writeln!(output, "ms per token:  {millis:.2}")?;
    }
    write_size_report(output, &summary.time_by_prompt_size)?;
    if let Some(mean) = summary.mean_log_probability {
        writeln!(output, "log prob:      {mean:.3}")?;
    }
    write_word_report(output, summary)?;
    write_reference_report(output, &summary.references)?;
    comparison.map_or(Ok(()), |comparison| write_comparison(output, comparison))
}

/// Writes the median time of the cases in each size group of their prompts.
fn write_size_report(output: &mut impl Write, groups: &[SizeTime]) -> io::Result<()> {
    if groups.is_empty() {
        return Ok(());
    }
    writeln!(output, "prompt tokens   cases   median time")?;
    for group in groups {
        let size = match group.max_tokens {
            Some(max_tokens) => format!("{} to {max_tokens}", group.min_tokens),
            None => format!("{} or more", group.min_tokens),
        };
        writeln!(
            output,
            "  {size:<12}  {:>5}   {} ms",
            group.cases, group.median_millis
        )?;
    }
    Ok(())
}

/// Writes the scores of each text that a replay types.
fn write_reference_report(output: &mut impl Write, references: &[ReferenceScore]) -> io::Result<()> {
    if references.is_empty() {
        return Ok(());
    }
    writeln!(output, "next word         partial word      replayed text")?;
    for reference in references {
        writeln!(
            output,
            "  {:<16}  {:<16}  {}",
            reference.next_word.to_string(),
            reference.partial_word.to_string(),
            reference.id
        )?;
    }
    Ok(())
}

/// Writes the cases that a change helped or harmed against the baseline with the p value of their
/// sign test, and the change of the log probability with its 95% interval.
fn write_comparison(output: &mut impl Write, comparison: &Comparison) -> io::Result<()> {
    writeln!(
        output,
        "helped:        {} {:?}",
        comparison.helped.len(),
        comparison.helped
    )?;
    writeln!(
        output,
        "harmed:        {} {:?}",
        comparison.harmed.len(),
        comparison.harmed
    )?;
    writeln!(output, "unchanged:     {}", comparison.unchanged)?;
    if let Some(p_value) = comparison.sign_test_p_value() {
        writeln!(output, "sign test:     p = {p_value:.3}")?;
    }
    let Some(change) = comparison.log_probability else {
        return Ok(());
    };
    let interval = change.margin.map_or_else(String::new, |margin| {
        format!(" [{:+.3}, {:+.3}]", change.mean - margin, change.mean + margin)
    });
    writeln!(
        output,
        "log prob diff: {:+.3}{interval} (higher {}, lower {})",
        change.mean, change.higher, change.lower
    )
}

/// Writes the words that show, and what a minimum for the first word and a cut of the tail do.
fn write_word_report(output: &mut impl Write, summary: &Summary) -> io::Result<()> {
    writeln!(
        output,
        "words:         {} right, {} wrong",
        summary.words.right, summary.words.wrong
    )?;
    if let Some(mean) = summary.mean_first_word_log_probability {
        writeln!(output, "first word:    log prob {mean:.3}")?;
    }
    writeln!(output, "first word min  kept    match   matches")?;
    for point in &summary.gate {
        writeln!(
            output,
            "  {:>5.1}%       {:>5.1}%  {:>5.1}%  {}/{}",
            point.log_probability.exp() * 100.0,
            percent(point.kept, point.suggestions),
            percent(point.kept_matches, point.kept),
            point.kept_matches,
            point.matches
        )?;
    }
    writeln!(output, "cut             shown   right   wrong")?;
    for point in &summary.cut {
        writeln!(
            output,
            "  {:>6.2}%      {:>5}   {:>5}   {:>5}",
            point.log_probability.exp() * 100.0,
            point.shown,
            point.right,
            point.wrong
        )?;
    }
    writeln!(output, "typo cut        caught  hidden  matches hidden")?;
    for point in &summary.typo_cut {
        writeln!(
            output,
            "  {:>5.1}         {:>2}/{:<3}  {:>3}/{:<4} {}",
            point.log_probability,
            point.caught,
            point.typos,
            point.hidden,
            point.others,
            point.hidden_matches
        )?;
    }
    Ok(())
}

fn percent(part: usize, whole: usize) -> f64 {
    let (Ok(part), Ok(whole)) = (u32::try_from(part), u32::try_from(whole)) else {
        return 0.0;
    };
    if whole == 0 {
        return 0.0;
    }
    f64::from(part) * 100.0 / f64::from(whole)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::test_outcomes::scored;

    #[test]
    fn comparison_shows_the_sign_test_and_the_interval_of_the_log_probability_change() {
        let baseline = [scored("small", false, -4.0), scored("large", false, -4.0)];
        let outcomes = [scored("small", true, -3.0), scored("large", true, -1.0)];
        let mut output = Vec::new();

        write_comparison(&mut output, &Comparison::of(&baseline, &outcomes)).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "helped:        2 [\"small\", \"large\"]\n\
             harmed:        0 []\n\
             unchanged:     0\n\
             sign test:     p = 0.500\n\
             log prob diff: +2.000 [+0.040, +3.960] (higher 2, lower 0)\n"
        );
    }
}
