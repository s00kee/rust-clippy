//! Lints concerned with the grouping of digits with underscores in integral or
//! floating-point literal expressions.

use crate::utils::{
    in_macro,
    numeric_literal::{NumericLiteral, Radix},
    snippet_opt, span_lint_and_sugg,
};
use if_chain::if_chain;
use rustc_ast::ast::{Expr, ExprKind, Lit, LitKind};
use rustc_errors::Applicability;
use rustc_lint::{EarlyContext, EarlyLintPass, LintContext};
use rustc_middle::lint::in_external_macro;
use rustc_session::{declare_lint_pass, declare_tool_lint, impl_lint_pass};

declare_clippy_lint! {
    /// **What it does:** Warns if a long integral or floating-point constant does
    /// not contain underscores.
    ///
    /// **Why is this bad?** Reading long numbers is difficult without separators.
    ///
    /// **Known problems:** None.
    ///
    /// **Example:**
    ///
    /// ```rust
    /// let x: u64 = 61864918973511;
    /// ```
    pub UNREADABLE_LITERAL,
    style,
    "long integer literal without underscores"
}

declare_clippy_lint! {
    /// **What it does:** Warns for mistyped suffix in literals
    ///
    /// **Why is this bad?** This is most probably a typo
    ///
    /// **Known problems:**
    /// - Recommends a signed suffix, even though the number might be too big and an unsigned
    ///   suffix is required
    /// - Does not match on `_127` since that is a valid grouping for decimal and octal numbers
    ///
    /// **Example:**
    ///
    /// ```rust
    /// 2_32;
    /// ```
    pub MISTYPED_LITERAL_SUFFIXES,
    correctness,
    "mistyped literal suffix"
}

declare_clippy_lint! {
    /// **What it does:** Warns if an integral or floating-point constant is
    /// grouped inconsistently with underscores.
    ///
    /// **Why is this bad?** Readers may incorrectly interpret inconsistently
    /// grouped digits.
    ///
    /// **Known problems:** None.
    ///
    /// **Example:**
    ///
    /// ```rust
    /// let x: u64 = 618_64_9189_73_511;
    /// ```
    pub INCONSISTENT_DIGIT_GROUPING,
    style,
    "integer literals with digits grouped inconsistently"
}

declare_clippy_lint! {
    /// **What it does:** Warns if the digits of an integral or floating-point
    /// constant are grouped into groups that
    /// are too large.
    ///
    /// **Why is this bad?** Negatively impacts readability.
    ///
    /// **Known problems:** None.
    ///
    /// **Example:**
    ///
    /// ```rust
    /// let x: u64 = 6186491_8973511;
    /// ```
    pub LARGE_DIGIT_GROUPS,
    pedantic,
    "grouping digits into groups that are too large"
}

declare_clippy_lint! {
    /// **What it does:** Warns if there is a better representation for a numeric literal.
    ///
    /// **Why is this bad?** Especially for big powers of 2 a hexadecimal representation is more
    /// readable than a decimal representation.
    ///
    /// **Known problems:** None.
    ///
    /// **Example:**
    ///
    /// `255` => `0xFF`
    /// `65_535` => `0xFFFF`
    /// `4_042_322_160` => `0xF0F0_F0F0`
    pub DECIMAL_LITERAL_REPRESENTATION,
    restriction,
    "using decimal representation when hexadecimal would be better"
}

enum WarningType {
    UnreadableLiteral,
    InconsistentDigitGrouping,
    LargeDigitGroups,
    DecimalRepresentation,
    MistypedLiteralSuffix,
}

impl WarningType {
    fn display(&self, suggested_format: String, cx: &EarlyContext<'_>, span: rustc_span::Span) {
        match self {
            Self::MistypedLiteralSuffix => span_lint_and_sugg(
                cx,
                MISTYPED_LITERAL_SUFFIXES,
                span,
                "mistyped literal suffix",
                "did you mean to write",
                suggested_format,
                Applicability::MaybeIncorrect,
            ),
            Self::UnreadableLiteral => span_lint_and_sugg(
                cx,
                UNREADABLE_LITERAL,
                span,
                "long literal lacking separators",
                "consider",
                suggested_format,
                Applicability::MachineApplicable,
            ),
            Self::LargeDigitGroups => span_lint_and_sugg(
                cx,
                LARGE_DIGIT_GROUPS,
                span,
                "digit groups should be smaller",
                "consider",
                suggested_format,
                Applicability::MachineApplicable,
            ),
            Self::InconsistentDigitGrouping => span_lint_and_sugg(
                cx,
                INCONSISTENT_DIGIT_GROUPING,
                span,
                "digits grouped inconsistently by underscores",
                "consider",
                suggested_format,
                Applicability::MachineApplicable,
            ),
            Self::DecimalRepresentation => span_lint_and_sugg(
                cx,
                DECIMAL_LITERAL_REPRESENTATION,
                span,
                "integer literal has a better hexadecimal representation",
                "consider",
                suggested_format,
                Applicability::MachineApplicable,
            ),
        };
    }
}

declare_lint_pass!(LiteralDigitGrouping => [
    UNREADABLE_LITERAL,
    INCONSISTENT_DIGIT_GROUPING,
    LARGE_DIGIT_GROUPS,
    MISTYPED_LITERAL_SUFFIXES,
]);

impl EarlyLintPass for LiteralDigitGrouping {
    fn check_expr(&mut self, cx: &EarlyContext<'_>, expr: &Expr) {
        if in_external_macro(cx.sess(), expr.span) {
            return;
        }

        if let ExprKind::Lit(ref lit) = expr.kind {
            Self::check_lit(cx, lit)
        }
    }
}

impl LiteralDigitGrouping {
    fn check_lit(cx: &EarlyContext<'_>, lit: &Lit) {
        if_chain! {
            if let Some(src) = snippet_opt(cx, lit.span);
            if let Some(mut num_lit) = NumericLiteral::from_lit(&src, &lit);
            then {
                if !Self::check_for_mistyped_suffix(cx, lit.span, &mut num_lit) {
                    return;
                }

                let result = (|| {

                    let integral_group_size = Self::get_group_size(num_lit.integer.split('_'))?;
                    if let Some(fraction) = num_lit.fraction {
                        let fractional_group_size = Self::get_group_size(fraction.rsplit('_'))?;

                        let consistent = Self::parts_consistent(integral_group_size,
                                                                fractional_group_size,
                                                                num_lit.integer.len(),
                                                                fraction.len());
                        if !consistent {
                            return Err(WarningType::InconsistentDigitGrouping);
                        };
                    }
                    Ok(())
                })();


                if let Err(warning_type) = result {
                    let should_warn = match warning_type {
                        | WarningType::UnreadableLiteral
                        | WarningType::InconsistentDigitGrouping
                        | WarningType::LargeDigitGroups => {
                            !in_macro(lit.span)
                        }
                        WarningType::DecimalRepresentation | WarningType::MistypedLiteralSuffix => {
                            true
                        }
                    };
                    if should_warn {
                        warning_type.display(num_lit.format(), cx, lit.span)
                    }
                }
            }
        }
    }

    // Returns `false` if the check fails
    fn check_for_mistyped_suffix(
        cx: &EarlyContext<'_>,
        span: rustc_span::Span,
        num_lit: &mut NumericLiteral<'_>,
    ) -> bool {
        if num_lit.suffix.is_some() {
            return true;
        }

        let (part, mistyped_suffixes, missing_char) = if let Some((_, exponent)) = &mut num_lit.exponent {
            (exponent, &["32", "64"][..], 'f')
        } else if let Some(fraction) = &mut num_lit.fraction {
            (fraction, &["32", "64"][..], 'f')
        } else {
            (&mut num_lit.integer, &["8", "16", "32", "64"][..], 'i')
        };

        let mut split = part.rsplit('_');
        let last_group = split.next().expect("At least one group");
        if split.next().is_some() && mistyped_suffixes.contains(&last_group) {
            *part = &part[..part.len() - last_group.len()];
            let mut sugg = num_lit.format();
            sugg.push('_');
            sugg.push(missing_char);
            sugg.push_str(last_group);
            WarningType::MistypedLiteralSuffix.display(sugg, cx, span);
            false
        } else {
            true
        }
    }

    /// Given the sizes of the digit groups of both integral and fractional
    /// parts, and the length
    /// of both parts, determine if the digits have been grouped consistently.
    #[must_use]
    fn parts_consistent(
        int_group_size: Option<usize>,
        frac_group_size: Option<usize>,
        int_size: usize,
        frac_size: usize,
    ) -> bool {
        match (int_group_size, frac_group_size) {
            // No groups on either side of decimal point - trivially consistent.
            (None, None) => true,
            // Integral part has grouped digits, fractional part does not.
            (Some(int_group_size), None) => frac_size <= int_group_size,
            // Fractional part has grouped digits, integral part does not.
            (None, Some(frac_group_size)) => int_size <= frac_group_size,
            // Both parts have grouped digits. Groups should be the same size.
            (Some(int_group_size), Some(frac_group_size)) => int_group_size == frac_group_size,
        }
    }

    /// Returns the size of the digit groups (or None if ungrouped) if successful,
    /// otherwise returns a `WarningType` for linting.
    fn get_group_size<'a>(groups: impl Iterator<Item = &'a str>) -> Result<Option<usize>, WarningType> {
        let mut groups = groups.map(str::len);

        let first = groups.next().expect("At least one group");

        if let Some(second) = groups.next() {
            if !groups.all(|x| x == second) || first > second {
                Err(WarningType::InconsistentDigitGrouping)
            } else if second > 4 {
                Err(WarningType::LargeDigitGroups)
            } else {
                Ok(Some(second))
            }
        } else if first > 5 {
            Err(WarningType::UnreadableLiteral)
        } else {
            Ok(None)
        }
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone)]
pub struct DecimalLiteralRepresentation {
    threshold: u64,
}

impl_lint_pass!(DecimalLiteralRepresentation => [DECIMAL_LITERAL_REPRESENTATION]);

impl EarlyLintPass for DecimalLiteralRepresentation {
    fn check_expr(&mut self, cx: &EarlyContext<'_>, expr: &Expr) {
        if in_external_macro(cx.sess(), expr.span) {
            return;
        }

        if let ExprKind::Lit(ref lit) = expr.kind {
            self.check_lit(cx, lit)
        }
    }
}

impl DecimalLiteralRepresentation {
    #[must_use]
    pub fn new(threshold: u64) -> Self {
        Self { threshold }
    }
    fn check_lit(self, cx: &EarlyContext<'_>, lit: &Lit) {
        // Lint integral literals.
        if_chain! {
            if let LitKind::Int(val, _) = lit.kind;
            if let Some(src) = snippet_opt(cx, lit.span);
            if let Some(num_lit) = NumericLiteral::from_lit(&src, &lit);
            if num_lit.radix == Radix::Decimal;
            if val >= u128::from(self.threshold);
            then {
                let hex = format!("{:#X}", val);
                let num_lit = NumericLiteral::new(&hex, num_lit.suffix, false);
                let _ = Self::do_lint(num_lit.integer).map_err(|warning_type| {
                    warning_type.display(num_lit.format(), cx, lit.span)
                });
            }
        }
    }

    fn do_lint(digits: &str) -> Result<(), WarningType> {
        if digits.len() == 1 {
            // Lint for 1 digit literals, if someone really sets the threshold that low
            if digits == "1"
                || digits == "2"
                || digits == "4"
                || digits == "8"
                || digits == "3"
                || digits == "7"
                || digits == "F"
            {
                return Err(WarningType::DecimalRepresentation);
            }
        } else if digits.len() < 4 {
            // Lint for Literals with a hex-representation of 2 or 3 digits
            let f = &digits[0..1]; // first digit
            let s = &digits[1..]; // suffix

            // Powers of 2
            if ((f.eq("1") || f.eq("2") || f.eq("4") || f.eq("8")) && s.chars().all(|c| c == '0'))
                // Powers of 2 minus 1
                || ((f.eq("1") || f.eq("3") || f.eq("7") || f.eq("F")) && s.chars().all(|c| c == 'F'))
            {
                return Err(WarningType::DecimalRepresentation);
            }
        } else {
            // Lint for Literals with a hex-representation of 4 digits or more
            let f = &digits[0..1]; // first digit
            let m = &digits[1..digits.len() - 1]; // middle digits, except last
            let s = &digits[1..]; // suffix

            // Powers of 2 with a margin of +15/-16
            if ((f.eq("1") || f.eq("2") || f.eq("4") || f.eq("8")) && m.chars().all(|c| c == '0'))
                || ((f.eq("1") || f.eq("3") || f.eq("7") || f.eq("F")) && m.chars().all(|c| c == 'F'))
                // Lint for representations with only 0s and Fs, while allowing 7 as the first
                // digit
                || ((f.eq("7") || f.eq("F")) && s.chars().all(|c| c == '0' || c == 'F'))
            {
                return Err(WarningType::DecimalRepresentation);
            }
        }

        Ok(())
    }
}
