use crate::BigNum;
use serde::{Deserialize, Serialize};
use serde_json::{value::Value as SerdeValue, Number};
use std::{convert::TryFrom, fmt, ops::{Div, Mul, Rem, Add, Sub}, str::FromStr};

pub type Map = serde_json::value::Map<String, SerdeValue>;

use super::{Input, Output};

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    TypeError,
    UnknownVariable,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TypeError => write!(f, "TypeError: Wrong type"),
            Error::UnknownVariable => write!(f, "UnknownVariable: Unknown variable passed"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Rule {
    Function(Function),
    Value(Value),
}

impl Rule {
    pub fn eval(&self, input: &Input, output: &mut Output) -> Result<Option<Value>, Error> {
        eval(input, output, self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "SerdeValue")]
pub enum Value {
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    BigNum(BigNum),
}

impl Value {
    pub fn new_string(string: &str) -> Self {
        Self::String(string.to_string())
    }

    pub fn new_number(number: impl Into<Number>) -> Self {
        Self::Number(number.into())
    }
}

impl TryFrom<SerdeValue> for Value {
    type Error = Error;

    fn try_from(serde_value: SerdeValue) -> Result<Self, Self::Error> {
        match serde_value {
            SerdeValue::Bool(bool) => Ok(Self::Bool(bool)),
            SerdeValue::Number(number) => Ok(Self::Number(number)),
            // It's impossible to have a BigNumber literal in the rules, since they're JSON based (conform to serde_json::value::Value)
            // However it is possible to obtain a BigNumber by invoking the Function::Bn
            SerdeValue::String(string) => Ok(Value::String(string)),
            SerdeValue::Array(serde_array) => {
                let array = serde_array
                    .into_iter()
                    .map(Value::try_from)
                    .collect::<Result<_, _>>()?;
                Ok(Self::Array(array))
            }
            SerdeValue::Object(_) | SerdeValue::Null => Err(Error::TypeError),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
// TODO: https://github.com/AdExNetwork/adex-validator-stack-rust/issues/296
pub enum Function {
    Div(Box<Rule>, Box<Rule>),
    Mul(Box<Rule>, Box<Rule>),
    Mod(Box<Rule>, Box<Rule>),
    Add(Box<Rule>, Box<Rule>),
    Sub(Box<Rule>, Box<Rule>),
    Max(Box<Rule>, Box<Rule>),
    Min(Box<Rule>, Box<Rule>),
    If(Box<Rule>, Box<Rule>),
    And(Box<Rule>, Box<Rule>),
    Intersects(Box<Rule>, Box<Rule>),
    Get(String),
    /// Output variables can be set any number of times by different rules, except `show`
    /// if `show` is at any point set to `false`, we stop executing rules and don't show the ad.
    Set(String, Box<Rule>),
    /// Bn(Value) function.
    Bn(Value),
}

impl From<Function> for Rule {
    fn from(function: Function) -> Self {
        Self::Function(function)
    }
}

impl From<Value> for Rule {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl Function {
    pub fn new_if(condition: impl Into<Rule>, then: impl Into<Rule>) -> Self {
        Self::If(Box::new(condition.into()), Box::new(then.into()))
    }

    pub fn new_and(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::And(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_intersects(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Intersects(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_get(key: &str) -> Self {
        Self::Get(key.to_string())
    }

    pub fn new_set(key: &str, eval: impl Into<Rule>) -> Self {
        Self::Set(key.to_string(), Box::new(eval.into()))
    }

    pub fn new_bn(value: impl Into<Value>) -> Self {
        Self::Bn(value.into())
    }
}

impl Value {
    pub fn try_bool(self) -> Result<bool, Error> {
        match self {
            Self::Bool(b) => Ok(b),
            _ => Err(Error::TypeError),
        }
    }

    pub fn try_array(self) -> Result<Vec<Value>, Error> {
        match self {
            Self::Array(array) => Ok(array),
            _ => Err(Error::TypeError),
        }
    }

    pub fn try_bignum(self) -> Result<BigNum, Error> {
        BigNum::try_from(self)
    }

    pub fn try_number(self) -> Result<Number, Error> {
        match self {
            Value::Number(number) => Ok(number),
            _ => Err(Error::TypeError),
        }
    }
}

impl TryFrom<Value> for BigNum {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(string) => BigNum::from_str(&string).map_err(|_| Error::TypeError),
            Value::BigNum(big_num) => Ok(big_num),
            Value::Number(number) => {
                BigNum::from_str(&number.to_string()).map_err(|_| Error::TypeError)
            }
            _ => Err(Error::TypeError),
        }
    }
}

/// Evaluates a Rule to be applied and has 3 outcomes:
/// - Does nothing
///     Rules returned directly:
///     - Bool
///     - Number
///     - String
///     - Array
///     - BigNum
/// - Mutates output
/// - Throws an error
fn eval(input: &Input, output: &mut Output, rule: &Rule) -> Result<Option<Value>, Error> {
    let function = match rule {
        Rule::Value(value) => return Ok(Some(value.clone())),
        Rule::Function(function) => function,
    };

    // basic operators
    let value = match function {
        Function::Div(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;


            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), second_value) => {
                    let second_bignum = BigNum::try_from(second_value)?;

                    Value::BigNum(bignum.div(second_bignum))
                },
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.div(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Division)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Mul(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.mul(rhs_bignum))
                },
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.mul(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Multiplication)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Mod(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.rem(rhs_bignum))
                },
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.rem(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Modulus)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Add(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.add(rhs_bignum))
                },
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.add(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Addition)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Sub(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.sub(rhs_bignum))
                },
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.sub(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Subtraction)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Max(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.max(rhs_bignum))
                },
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.max(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Max)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Min(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.min(rhs_bignum))
                },
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.min(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Min)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::If(first_rule, second_rule) => {
            let eval_if = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            if eval_if {
                eval(input, output, second_rule)?
            } else {
                None
            }
        }
        Function::And(first_rule, second_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;
            let b = eval(input, output, second_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            Some(Value::Bool(a && b))
        }
        Function::Intersects(first_rule, second_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_array()?;
            let b = eval(input, output, second_rule)?
                .ok_or(Error::TypeError)?
                .try_array()?;

            Some(Value::Bool(a.iter().any(|x| b.contains(x))))
        }
        Function::Set(key, rule) => {
            // Output variables can be set any number of times by different rules, except `show`
            // if `show` is at any point set to `false`, we stop executing rules and don't show the ad.
            match key.as_str() {
                "boost" => {
                    let boost_num = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_number()?;

                    output.boost = boost_num.as_f64().ok_or(Error::TypeError)?;
                }
                "show" => {
                    let show_value = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_bool()?;

                    output.show = show_value;
                }
                "price.IMPRESSION" => {
                    let price = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_bignum()?;

                    // we do not care about any other old value
                    output.price.insert("IMPRESSION".to_string(), price);
                }
                "price.CLICK" => {
                    let price = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_bignum()?;

                    // we do not care about any other old value
                    output.price.insert("CLICK".to_string(), price);
                }
                _ => return Err(Error::UnknownVariable),
            }

            return Ok(None);
        }
        Function::Get(key) => Some(input.try_get(key)?),
        Function::Bn(value) => {
            let big_num = value.clone().try_bignum()?;

            Some(Value::BigNum(big_num))
        }
    };

    Ok(value)
}

enum MathOperator {
    Division,
    Multiplication,
    Modulus,
    Addition,
    Subtraction,
    Max,
    Min,
}

fn handle_u64(lhs: u64, rhs: u64, ops: MathOperator) -> Result<Number, Error> {
        match ops {
            MathOperator::Division => {
                let divided = lhs.checked_div(rhs).ok_or(Error::TypeError)?;
                Ok(divided.into())
            },
            MathOperator::Multiplication => {
                let multiplied = lhs.checked_mul(rhs).ok_or(Error::TypeError)?;
                Ok(multiplied.into())
            },
            MathOperator::Modulus => {
                let modulus = lhs.checked_rem(rhs).ok_or(Error::TypeError)?;
                Ok(modulus.into())
            },
            MathOperator::Addition => {
                let added = lhs.checked_add(rhs).ok_or(Error::TypeError)?;
                Ok(added.into())
            },
            MathOperator::Subtraction => {
                let subtracted = lhs.checked_sub(rhs).ok_or(Error::TypeError)?;
                Ok(subtracted.into())
            },
            MathOperator::Max => {
                let max = lhs.max(rhs);
                Ok(max.into())
            },
            MathOperator::Min => {
                let min = lhs.min(rhs);
                Ok(min.into())
            },
        }
}

fn handle_i64(lhs: i64, rhs: i64, ops: MathOperator) -> Result<Number, Error> {
    match ops {
        MathOperator::Division => {
            let divided = lhs.checked_div(rhs).ok_or(Error::TypeError)?;
            Ok(divided.into())
        },
        MathOperator::Multiplication => {
            let multiplied = lhs.checked_mul(rhs).ok_or(Error::TypeError)?;
            Ok(multiplied.into())
        },
        MathOperator::Modulus => {
            let modulus = lhs.checked_rem(rhs).ok_or(Error::TypeError)?;
            Ok(modulus.into())
        },
        MathOperator::Addition => {
            let added = lhs.checked_add(rhs).ok_or(Error::TypeError)?;
            Ok(added.into())
        },
        MathOperator::Subtraction => {
            let subtracted = lhs.checked_sub(rhs).ok_or(Error::TypeError)?;
            Ok(subtracted.into())
        },
        MathOperator::Max => {
            let max = lhs.max(rhs);
            Ok(max.into())
        },
        MathOperator::Min => {
            let min = lhs.min(rhs);
            Ok(min.into())
        },
    }
}

fn handle_f64(lhs: f64, rhs: f64, ops:MathOperator) -> Result<Number, Error> {
        match ops {
            MathOperator::Division => {
                let divided = lhs.div(rhs);
                Ok(Number::from_f64(divided).ok_or(Error::TypeError)?)
            },
            MathOperator::Multiplication => {
                let multiplied = lhs.mul(rhs);
                Ok(Number::from_f64(multiplied).ok_or(Error::TypeError)?)
            },
            MathOperator::Modulus => {
                let modulus = lhs.rem(rhs);
                Ok(Number::from_f64(modulus).ok_or(Error::TypeError)?)
            },
            MathOperator::Addition => {
                let added = lhs.add(rhs);
                Ok(Number::from_f64(added).ok_or(Error::TypeError)?)
            },
            MathOperator::Subtraction => {
                let subtracted = lhs.sub(rhs);
                Ok(Number::from_f64(subtracted).ok_or(Error::TypeError)?)
            },
            MathOperator::Max => {
                let max = lhs.max(rhs);
                Ok(Number::from_f64(max).ok_or(Error::TypeError)?)
            },
            MathOperator::Min => {
                let min = lhs.min(rhs);
                Ok(Number::from_f64(min).ok_or(Error::TypeError)?)
            },
        }
}

fn math_operator(lhs: Number, rhs: Number, ops: MathOperator) -> Result<Number, Error> {
    match (lhs.as_u64(), rhs.as_u64()) {
        (Some(lhs), Some(rhs)) => handle_u64(lhs, rhs, ops),
        _ => {
            match (lhs.as_i64(), rhs.as_i64()) {
                (Some(lhs), Some(rhs)) => handle_i64(lhs, rhs, ops),
                _ => {
                    match (lhs.as_f64(), rhs.as_f64()) {
                        (Some(lhs), Some(rhs)) => handle_f64(lhs, rhs, ops),
                        _ => Err(Error::TypeError)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::targeting::AdSlot;

    #[test]
    fn deserialize_intersects_with_get_rule() {
        let json = r#"{"intersects": [{ "get": "adSlot.categories" }, ["News", "Bitcoin"]]}"#;

        let parsed_rule = serde_json::from_str::<Rule>(json).expect("Should deserialize");

        let expected = Rule::Function(Function::new_intersects(
            Rule::Function(Function::new_get("adSlot.categories")),
            Rule::Value(Value::Array(vec![
                Value::new_string("News"),
                Value::new_string("Bitcoin"),
            ])),
        ));

        assert_eq!(expected, parsed_rule)
    }

    /// ```json
    /// {
    ///   "intersects": [
    ///     {
    ///       "get": "adSlot.categories"
    ///     },
    ///     [
    ///       "News",
    ///       "Bitcoin"
    ///     ]
    ///   ]
    /// }
    /// ```
    #[test]
    fn test_intersects_eval() {
        let mut input = Input::default();
        input.ad_slot = Some(AdSlot {
            categories: vec!["Bitcoin".to_string(), "Ethereum".to_string()],
            hostname: Default::default(),
            alexa_rank: 0.0,
        });

        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let categories = vec![Value::new_string("News"), Value::new_string("Bitcoin")];

        let rules = Rule::Function(Function::new_intersects(
            Function::new_get("adSlot.categories"),
            Value::Array(categories),
        ));

        let result = rules.eval(&input, &mut output).expect("Should eval rules");

        assert_eq!(
            Value::Bool(true),
            result.expect("Should return Non-NULL result!")
        );

        let mut input = Input::default();
        input.ad_slot = Some(AdSlot {
            categories: vec!["Advertisement".to_string(), "Programming".to_string()],
            hostname: Default::default(),
            alexa_rank: 0.0,
        });

        let result = rules.eval(&input, &mut output).expect("Should eval rules");

        assert_eq!(
            Value::Bool(false),
            result.expect("Should return Non-NULL result!")
        );
    }

    #[test]
    fn test_and_eval() {
        let input = Input::default();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (true, true, true),
            (false, false, false),
            (false, true, false),
            (true, false, false),
        ];

        for (lhs, rhs, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_and(Value::Bool(*lhs), Value::Bool(*rhs)));
            let expected = Some(Value::Bool(*expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }

    #[test]
    fn test_if_eval() {
        let input = Input::default();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let then = Value::String("yes".to_string());

        let rule = Rule::Function(Function::new_if(Value::Bool(true), then.clone()));

        assert_eq!(Ok(Some(then.clone())), rule.eval(&input, &mut output));

        let rule = Rule::Function(Function::new_if(Value::Bool(false), then));

        assert_eq!(Ok(None), rule.eval(&input, &mut output));
    }

    #[test]
    fn test_bn_eval_from_actual_number_value_string_bignum_or_number() {
        let input = Input::default();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (Value::new_string("1000"), Value::BigNum(1000.into())),
            (Value::new_number(2_000), Value::BigNum(2_000.into())),
            (Value::BigNum(3.into()), Value::BigNum(3.into())),
            // rounded floats should work!
            (
                Value::Number(Number::from_f64(40.0).expect("should create float number")),
                Value::BigNum(40.into()),
            ),
        ];

        for (from, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_bn(from));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }

    #[test]
    fn test_bn_eval_from_actual_incorrect_value() {
        let input = Input::default();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let error_cases = vec![
            Value::new_string("text"),
            // BigNums can only be positive
            Value::new_number(-100),
            Value::Bool(true),
            Value::Array(vec![Value::Bool(false)]),
            Value::Number(Number::from_f64(2.5).expect("should create float number")),
        ];

        for error_case in error_cases.into_iter() {
            let rule = Rule::Function(Function::new_bn(error_case));

            assert_eq!(Err(Error::TypeError), rule.eval(&input, &mut output));
        }
    }

    #[test]
    fn test_set_eval() {
        use crate::channel::{Pricing, PricingBounds};
        use crate::util::tests::prep_db::DUMMY_CHANNEL;

        let mut channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            impression: Some(Pricing {
                min: 1_000.into(),
                max: 2_000.into(),
            }),
            click: Some(Pricing {
                min: 3_000.into(),
                max: 4_000.into(),
            }),
        });

        let input = Input::default();
        let mut output = Output::from(&channel);

        assert_eq!(Some(&BigNum::from(1_000)), output.price.get("IMPRESSION"));

        let set_to = Value::BigNum(BigNum::from(20));
        let rule = Rule::Function(Function::new_set("price.IMPRESSION", set_to));

        assert_eq!(Ok(None), rule.eval(&input, &mut output));

        assert_eq!(Some(&BigNum::from(20)), output.price.get("IMPRESSION"));
    }
}
