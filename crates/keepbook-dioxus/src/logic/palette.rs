/// Defines `fn $name(index: usize) -> &'static str` over the numbered CSS
/// custom-property palette `--$prefix-$slot`, wrapping the index into range.
/// Spending tags and stacked chart series both colour by position this way.
macro_rules! css_var_palette {
    ($vis:vis fn $name:ident from $prefix:literal [$($slot:literal),+ $(,)?]) => {
        $vis fn $name(index: usize) -> &'static str {
            const PALETTE: &[&str] = &[$(concat!("var(--", $prefix, "-", $slot, ")")),+];
            PALETTE[index % PALETTE.len()]
        }
    };
}

pub(crate) use css_var_palette;
