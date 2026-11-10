//! Normalization of extracted items so that equivalent descriptions hash to the same cache key.
//!
//! Deliberately conservative: only formatting differences and words that never change carbs are
//! collapsed. Words like "cooked" or "dried" (for fruit) are kept because they change carbs per
//! serving.

use crate::models::ExtractedItem;

/// Words dropped from item names; they carry no nutritional meaning.
const FILLER_WORDS: &[&str] = &["a", "an", "the", "some", "of", "fresh", "plain"];

/// Words dropped from item names only when the food is normally measured dry (so
/// "dried oatmeal" and "oatmeal" match, but "dried apricot" stays distinct from "apricot").
const DRY_STATE_WORDS: &[&str] = &["dry", "dried", "uncooked", "raw"];
const DRY_MEASURED_FOODS: &[&str] = &[
    "oatmeal", "oat", "rice", "pasta", "quinoa", "couscous", "lentil", "barley",
];

/// Names that end in "s" but are not plurals.
const NOT_PLURAL: &[&str] = &["hummus", "asparagus", "molasses", "couscous", "swiss", "citrus"];

/// Name aliases applied after cleanup.
const NAME_ALIASES: &[(&str, &str)] = &[("rolled oat", "oatmeal"), ("oat", "oatmeal")];

const UNIT_SYNONYMS: &[(&str, &str)] = &[
    ("teaspoon", "tsp"),
    ("teaspoons", "tsp"),
    ("tsp", "tsp"),
    ("tsps", "tsp"),
    ("tablespoon", "tbsp"),
    ("tablespoons", "tbsp"),
    ("tbsp", "tbsp"),
    ("tbsps", "tbsp"),
    ("tbs", "tbsp"),
    ("cup", "cup"),
    ("cups", "cup"),
    ("c", "cup"),
    ("fl oz", "fl oz"),
    ("floz", "fl oz"),
    ("fluid ounce", "fl oz"),
    ("fluid ounces", "fl oz"),
    ("ml", "ml"),
    ("milliliter", "ml"),
    ("milliliters", "ml"),
    ("millilitre", "ml"),
    ("millilitres", "ml"),
    ("l", "l"),
    ("liter", "l"),
    ("liters", "l"),
    ("litre", "l"),
    ("litres", "l"),
    ("oz", "oz"),
    ("ounce", "oz"),
    ("ounces", "oz"),
    ("lb", "lb"),
    ("lbs", "lb"),
    ("pound", "lb"),
    ("pounds", "lb"),
    ("g", "g"),
    ("gram", "g"),
    ("grams", "g"),
    ("kg", "kg"),
    ("kilogram", "kg"),
    ("kilograms", "kg"),
    ("individual", "individual"),
    ("individuals", "individual"),
    ("each", "individual"),
    ("whole", "individual"),
    ("item", "individual"),
    ("items", "individual"),
    ("slice", "slice"),
    ("slices", "slice"),
    ("strip", "strip"),
    ("strips", "strip"),
    ("piece", "piece"),
    ("pieces", "piece"),
    ("pc", "piece"),
    ("pcs", "piece"),
];

pub fn normalize_item(item: &ExtractedItem) -> ExtractedItem {
    ExtractedItem {
        item: normalize_name(&item.item),
        quantity: normalize_quantity(&item.quantity),
        quantity_type: normalize_unit(&item.quantity_type),
    }
}

pub fn normalize_name(name: &str) -> String {
    // Drop parenthetical text, then anything that isn't a letter, digit or space.
    let mut cleaned = String::with_capacity(name.len());
    let mut depth = 0u32;
    for ch in name.to_lowercase().chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            c if c.is_alphanumeric() => cleaned.push(c),
            _ => cleaned.push(' '),
        }
    }

    let mut words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| !FILLER_WORDS.contains(w))
        .collect();

    if let Some(last) = words.last().copied() {
        let base = singularize(last);
        if DRY_MEASURED_FOODS.contains(&base.as_str()) {
            words.retain(|w| !DRY_STATE_WORDS.contains(w));
        }
    }

    let mut words: Vec<String> = words.into_iter().map(String::from).collect();
    if let Some(last) = words.last_mut() {
        *last = singularize(last);
    }

    let joined = words.join(" ");
    NAME_ALIASES
        .iter()
        .find(|(from, _)| *from == joined)
        .map(|(_, to)| to.to_string())
        .unwrap_or(joined)
}

fn singularize(word: &str) -> String {
    if NOT_PLURAL.contains(&word) || word.len() <= 3 {
        return word.to_string();
    }
    if let Some(stem) = word.strip_suffix("ies") {
        return format!("{stem}y");
    }
    if let Some(stem) = word.strip_suffix("oes") {
        return format!("{stem}o");
    }
    if word.ends_with("ss") || word.ends_with("us") || word.ends_with("is") {
        return word.to_string();
    }
    word.strip_suffix('s').unwrap_or(word).to_string()
}

pub fn normalize_quantity(quantity: &str) -> String {
    let q = quantity.trim().to_lowercase();
    match parse_quantity(&q) {
        Some(n) => format_number(n),
        None => q,
    }
}

fn parse_quantity(q: &str) -> Option<f64> {
    match q {
        "a" | "an" | "one" => return Some(1.0),
        "two" => return Some(2.0),
        "three" => return Some(3.0),
        "four" => return Some(4.0),
        "five" => return Some(5.0),
        "six" => return Some(6.0),
        "seven" => return Some(7.0),
        "eight" => return Some(8.0),
        "nine" => return Some(9.0),
        "ten" => return Some(10.0),
        "half" | "a half" => return Some(0.5),
        "quarter" | "a quarter" => return Some(0.25),
        _ => {}
    }

    // Mixed numbers like "1 1/2", or a single token.
    let parts: Vec<&str> = q.split_whitespace().collect();
    match parts.as_slice() {
        [one] => parse_token(one),
        [whole, frac] if frac.contains('/') => Some(parse_token(whole)? + parse_token(frac)?),
        _ => None,
    }
}

fn parse_token(t: &str) -> Option<f64> {
    if let Some((n, d)) = t.split_once('/') {
        let n: f64 = n.parse().ok()?;
        let d: f64 = d.parse().ok()?;
        return if d == 0.0 { None } else { Some(n / d) };
    }
    t.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn format_number(n: f64) -> String {
    let s = format!("{n:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn normalize_unit(unit: &str) -> String {
    let u = unit.trim().to_lowercase();
    let u = u.trim_end_matches('.');
    UNIT_SYNONYMS
        .iter()
        .find(|(from, _)| *from == u)
        .map(|(_, to)| to.to_string())
        .unwrap_or_else(|| u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, qty: &str, unit: &str) -> ExtractedItem {
        ExtractedItem {
            item: name.to_string(),
            quantity: qty.to_string(),
            quantity_type: unit.to_string(),
        }
    }

    #[test]
    fn issue_39_oatmeal_variants_normalize_equal() {
        let a = normalize_item(&item("dried oatmeal", "0.5", "cup"));
        let b = normalize_item(&item("Oatmeal", ".5", "cups"));
        let c = normalize_item(&item("rolled oats (dry)", "1/2", "Cup"));
        assert_eq!((&a.item, &a.quantity, &a.quantity_type), (&b.item, &b.quantity, &b.quantity_type));
        assert_eq!((&a.item, &a.quantity, &a.quantity_type), (&c.item, &c.quantity, &c.quantity_type));
        assert_eq!(a.item, "oatmeal");
        assert_eq!(a.quantity, "0.5");
        assert_eq!(a.quantity_type, "cup");
    }

    #[test]
    fn quantity_formats() {
        for q in [".5", "0.50", "1/2", "half", " 0.5 "] {
            assert_eq!(normalize_quantity(q), "0.5", "input {q:?}");
        }
        assert_eq!(normalize_quantity("1 1/2"), "1.5");
        assert_eq!(normalize_quantity("2"), "2");
        assert_eq!(normalize_quantity("two"), "2");
        assert_eq!(normalize_quantity("1/0"), "1/0");
        assert_eq!(normalize_quantity("a few"), "a few");
    }

    #[test]
    fn unit_synonyms() {
        assert_eq!(normalize_unit("Cups"), "cup");
        assert_eq!(normalize_unit("tablespoons"), "tbsp");
        assert_eq!(normalize_unit("teaspoon"), "tsp");
        assert_eq!(normalize_unit("ounces"), "oz");
        assert_eq!(normalize_unit("lbs"), "lb");
        assert_eq!(normalize_unit("liters"), "l");
        assert_eq!(normalize_unit("slices"), "slice");
        assert_eq!(normalize_unit("each"), "individual");
        assert_eq!(normalize_unit("handful"), "handful");
    }

    #[test]
    fn names_singularize_and_clean() {
        assert_eq!(normalize_name("Scrambled Eggs"), "scrambled egg");
        assert_eq!(normalize_name("berries"), "berry");
        assert_eq!(normalize_name("potatoes"), "potato");
        assert_eq!(normalize_name("hummus"), "hummus");
        assert_eq!(normalize_name("a slice of plain bread"), "slice bread");
    }

    #[test]
    fn carb_relevant_modifiers_are_kept() {
        assert_ne!(normalize_name("cooked rice"), normalize_name("rice"));
        assert_ne!(normalize_name("dried apricot"), normalize_name("apricot"));
        assert_ne!(normalize_name("brown rice"), normalize_name("rice"));
        assert_ne!(normalize_name("sweet potato"), normalize_name("potato"));
        assert_eq!(normalize_name("uncooked rice"), "rice");
    }
}
