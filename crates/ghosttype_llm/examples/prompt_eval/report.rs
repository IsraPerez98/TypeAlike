//! The report that the eval prints.

use std::io::{self, Write};

use crate::score::{Comparison, SizeTime, Summary};

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

/// Writes the cases that a change helped or harmed against the baseline, and the change of the log
/// probability.
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
    let Some(change) = comparison.log_probability else {
        return Ok(());
    };
    writeln!(
        output,
        "log prob diff: {:+.3} (higher {}, lower {})",
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
