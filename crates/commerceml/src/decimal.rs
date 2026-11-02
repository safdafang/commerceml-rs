//! Exact decimal numbers for amounts and quantities.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

/// An exact decimal number, as used by `СуммаТип` and `КоличествоТип`.
///
/// It stores a 128-bit mantissa and a decimal scale, so it never loses
/// precision and keeps the written form: `1.00` is displayed as `1.00`.
/// Comparison and hashing look at the value, so `1.0 == 1.00`.
///
/// ```
/// use commerceml::Decimal;
///
/// let price: Decimal = "1499.90".parse()?;
/// assert_eq!(price.to_string(), "1499.90");
/// assert_eq!(price, "1499.9".parse()?);
/// assert_eq!(price.to_f64(), 1499.9);
/// # Ok::<(), commerceml::ParseDecimalError>(())
/// ```
#[derive(Clone, Copy, Default)]
pub struct Decimal {
    mantissa: i128,
    scale: u32,
}

/// The maximum number of fractional digits.
const MAX_SCALE: u32 = 28;

/// Error returned when a string is not a valid decimal number.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid decimal number {0:?}")]
pub struct ParseDecimalError(pub String);

impl Decimal {
    /// Zero.
    pub const ZERO: Self = Self {
        mantissa: 0,
        scale: 0,
    };

    /// Creates `mantissa × 10^-scale`.
    ///
    /// # Panics
    ///
    /// Panics if `scale` is greater than 28.
    pub const fn new(mantissa: i128, scale: u32) -> Self {
        assert!(scale <= MAX_SCALE, "decimal scale must be at most 28");
        Self { mantissa, scale }
    }

    /// The integer mantissa.
    pub const fn mantissa(self) -> i128 {
        self.mantissa
    }

    /// The number of digits after the decimal point.
    pub const fn scale(self) -> u32 {
        self.scale
    }

    /// Whether the value is zero.
    pub const fn is_zero(self) -> bool {
        self.mantissa == 0
    }

    /// Whether the value is below zero.
    pub const fn is_negative(self) -> bool {
        self.mantissa < 0
    }

    /// Nearest `f64`. May lose precision.
    #[allow(clippy::cast_precision_loss)]
    pub fn to_f64(self) -> f64 {
        self.to_string().parse().unwrap_or(self.mantissa as f64)
    }

    /// The same value with trailing fractional zeros removed (`1.500` → `1.5`).
    #[must_use]
    pub fn normalize(self) -> Self {
        let mut d = self;
        while d.scale > 0 && d.mantissa % 10 == 0 {
            d.mantissa /= 10;
            d.scale -= 1;
        }
        d
    }

    /// Sum of two values, `None` on overflow.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let scale = self.scale.max(other.scale);
        let a = rescale(self.mantissa, self.scale, scale)?;
        let b = rescale(other.mantissa, other.scale, scale)?;
        Some(Self {
            mantissa: a.checked_add(b)?,
            scale,
        })
    }

    /// Product of two values, `None` on overflow or if the scale would exceed 28.
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        let scale = self.scale + other.scale;
        if scale > MAX_SCALE {
            return None;
        }
        Some(Self {
            mantissa: self.mantissa.checked_mul(other.mantissa)?,
            scale,
        })
    }

    /// Parses leniently: accepts `,` as the decimal separator and ignores
    /// spaces (including no-break spaces) used as digit group separators.
    pub fn parse_lenient(s: &str) -> Result<Self, ParseDecimalError> {
        let cleaned: String = s
            .trim()
            .chars()
            .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}'))
            .map(|c| if c == ',' { '.' } else { c })
            .collect();
        cleaned.parse().map_err(|_| ParseDecimalError(s.to_owned()))
    }
}

fn rescale(mantissa: i128, from: u32, to: u32) -> Option<i128> {
    mantissa.checked_mul(10i128.checked_pow(to - from)?)
}

impl FromStr for Decimal {
    type Err = ParseDecimalError;

    /// Parses the `xsd:decimal` lexical form: optional sign, digits, optional
    /// fraction. Surrounding whitespace is ignored.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseDecimalError(s.to_owned());
        let t = s.trim();
        let (negative, digits) = match t.as_bytes().first() {
            Some(b'-') => (true, &t[1..]),
            Some(b'+') => (false, &t[1..]),
            _ => (false, t),
        };
        let (int, frac) = match digits.split_once('.') {
            Some((i, f)) => (i, f),
            None => (digits, ""),
        };
        if int.is_empty() && frac.is_empty() {
            return Err(err());
        }
        if !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        let scale = u32::try_from(frac.len()).map_err(|_| err())?;
        if scale > MAX_SCALE {
            return Err(err());
        }
        let mut mantissa: i128 = 0;
        for b in int.bytes().chain(frac.bytes()) {
            mantissa = mantissa
                .checked_mul(10)
                .and_then(|m| m.checked_add(i128::from(b - b'0')))
                .ok_or_else(err)?;
        }
        if negative {
            mantissa = -mantissa;
        }
        Ok(Self { mantissa, scale })
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.mantissa.unsigned_abs().to_string();
        let scale = self.scale as usize;
        let sign = if self.mantissa < 0 { "-" } else { "" };
        if scale == 0 {
            return write!(f, "{sign}{digits}");
        }
        let padded = if digits.len() <= scale {
            format!("{}{digits}", "0".repeat(scale - digits.len() + 1))
        } else {
            digits
        };
        let (int, frac) = padded.split_at(padded.len() - scale);
        write!(f, "{sign}{int}.{frac}")
    }
}

impl fmt::Debug for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Decimal({self})")
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let scale = self.scale.max(other.scale);
        if let (Some(a), Some(b)) = (
            rescale(self.mantissa, self.scale, scale),
            rescale(other.mantissa, other.scale, scale),
        ) {
            return a.cmp(&b);
        }
        let (a, b) = (self.normalize(), other.normalize());
        let scale = a.scale.max(b.scale);
        if let (Some(x), Some(y)) = (
            rescale(a.mantissa, a.scale, scale),
            rescale(b.mantissa, b.scale, scale),
        ) {
            return x.cmp(&y);
        }
        a.to_f64().total_cmp(&b.to_f64())
    }
}

impl Hash for Decimal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let n = self.normalize();
        n.mantissa.hash(state);
        n.scale.hash(state);
    }
}

impl From<i64> for Decimal {
    fn from(v: i64) -> Self {
        Self {
            mantissa: i128::from(v),
            scale: 0,
        }
    }
}

impl From<u32> for Decimal {
    fn from(v: u32) -> Self {
        Self {
            mantissa: i128::from(v),
            scale: 0,
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Decimal {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Decimal {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = Decimal;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal number as a string or integer")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Decimal, E> {
                v.parse().map_err(E::custom)
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Decimal, E> {
                Ok(Decimal::from(v))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Decimal, E> {
                Ok(Decimal {
                    mantissa: i128::from(v),
                    scale: 0,
                })
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn parses_and_displays_lexical_form() {
        for s in ["0", "1.00", "-12.5", "0.001", "123456789.123456789", "-0.5"] {
            assert_eq!(d(s).to_string(), s);
        }
        assert_eq!(d("+7").to_string(), "7");
        assert_eq!(d(".5").to_string(), "0.5");
        assert_eq!(d("5.").to_string(), "5");
        assert_eq!(d(" 3.1 ").to_string(), "3.1");
    }

    #[test]
    fn rejects_garbage() {
        for s in [
            "",
            "-",
            ".",
            "1e5",
            "1.2.3",
            "abc",
            "1,5",
            "--1",
            "0.12345678901234567890123456789",
        ] {
            assert!(s.parse::<Decimal>().is_err(), "{s:?} should fail");
        }
        let too_long = "9".repeat(40);
        assert!(too_long.parse::<Decimal>().is_err());
    }

    #[test]
    fn lenient_parsing() {
        assert_eq!(Decimal::parse_lenient("1 234,50").unwrap().to_string(), "1234.50");
        assert_eq!(Decimal::parse_lenient("1\u{a0}000").unwrap().to_string(), "1000");
        assert!(Decimal::parse_lenient("abc").is_err());
    }

    #[test]
    fn value_equality_and_order() {
        assert_eq!(d("1.0"), d("1.00"));
        assert!(d("1.01") > d("1.009"));
        assert!(d("-2") < d("-1.5"));
        let mut h1 = std::collections::hash_map::DefaultHasher::new();
        let mut h2 = std::collections::hash_map::DefaultHasher::new();
        d("2.50").hash(&mut h1);
        d("2.5").hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn arithmetic() {
        assert_eq!(d("1.5").checked_add(d("2.25")).unwrap().to_string(), "3.75");
        assert_eq!(d("1.5").checked_mul(d("3")).unwrap().to_string(), "4.5");
        assert!(Decimal::new(i128::MAX, 0).checked_add(d("1")).is_none());
        assert_eq!(d("1.500").normalize().to_string(), "1.5");
        assert!((d("0.1").to_f64() - 0.1).abs() < f64::EPSILON);
        assert!(d("-1").is_negative() && d("0.00").is_zero());
        assert_eq!(Decimal::new(5, 2).to_string(), "0.05");
        assert_eq!(Decimal::from(7_i64).mantissa(), 7);
    }

    #[test]
    fn compare_with_huge_scale_difference() {
        let big = Decimal::new(i128::MAX / 10, 0);
        let small = d("0.0000000000000000000000000001");
        assert!(big > small);
    }
}
