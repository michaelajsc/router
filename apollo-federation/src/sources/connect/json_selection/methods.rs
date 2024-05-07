use apollo_compiler::collections::IndexMap;
use apollo_compiler::collections::IndexSet;
use lazy_static::lazy_static;
use serde_json_bytes::serde_json::Number;
use serde_json_bytes::Value as JSON;

use super::helpers::json_type_name;
use super::immutable::InputPath;
use super::js_literal::JSLiteral;
use super::ApplyTo;
use super::ApplyToError;
use super::MethodArgs;
use super::PathList;
use super::VarsWithPathsMap;

type ArrowMethod = fn(
    // Method name
    &str,
    // Arguments passed to this method
    &Option<MethodArgs>,
    // The JSON input value (data)
    &JSON,
    // The variables
    &VarsWithPathsMap,
    // The input_path (may contain integers)
    &InputPath<JSON>,
    // The rest of the PathList
    &PathList,
    // Errors
    &mut IndexSet<ApplyToError>,
) -> Option<JSON>;

lazy_static! {
    pub(super) static ref ARROW_METHODS: IndexMap<String, ArrowMethod> = {
        let mut methods = IndexMap::<String, ArrowMethod>::default();

        // This built-in method returns its first input argument as-is, ignoring
        // the input data. Useful for embedding literal values, as in
        // $->echo("give me this string").
        methods.insert("echo".to_string(), echo_method);

        // Returns the type of the data as a string, e.g. "object", "array",
        // "string", "number", "boolean", or "null". Note that `typeof null` is
        // "object" in JavaScript but "null" for our purposes.
        methods.insert("typeof".to_string(), typeof_method);

        // When invoked against an array, ->map evaluates its first argument
        // against each element of the array and returns an array of the
        // results. When invoked against a non-array, ->map evaluates its first
        // argument against the data and returns the result.
        methods.insert("map".to_string(), map_method);

        // Returns true if the data is deeply equal to the first argument, false
        // otherwise. Equality is solely value-based (all JSON), no references.
        methods.insert("eq".to_string(), eq_method);

        // Takes any number of pairs [candidate, value], and returns value for
        // the first candidate that equals the input data $. If none of the
        // pairs match, a runtime error is reported, but a single-element
        // [<default>] array as the final argument guarantees a default value.
        methods.insert("match".to_string(), match_method);

        // Like ->match, but expects the first element of each pair to evaluate
        // to a boolean, returning the second element of the first pair whose
        // first element is true. This makes providing a final catch-all case
        // easy, since the last pair can be [true, <default>].
        methods.insert("matchIf".to_string(), match_if_method);
        methods.insert("match_if".to_string(), match_if_method);

        // Arithmetic methods
        methods.insert("add".to_string(), add_method);
        methods.insert("sub".to_string(), sub_method);
        methods.insert("mul".to_string(), mul_method);
        methods.insert("div".to_string(), div_method);
        methods.insert("mod".to_string(), mod_method);

        // Array methods
        methods.insert("first".to_string(), first_method);
        methods.insert("last".to_string(), last_method);
        methods.insert("slice".to_string(), slice_method);

        // Logical methods
        methods.insert("not".to_string(), not_method);
        methods.insert("or".to_string(), or_method);
        methods.insert("and".to_string(), and_method);

        methods
    };
}

fn echo_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(args)) = method_args {
        if let Some(arg) = args.first() {
            return arg
                .apply_to_path(data, vars, input_path, errors)
                .and_then(|value| tail.apply_to_path(&value, vars, input_path, errors));
        }
    }
    errors.insert(ApplyToError::new(
        format!("Method ->{} requires one argument", method_name).as_str(),
        input_path.to_vec(),
    ));
    None
}

fn typeof_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(_)) = method_args {
        errors.insert(ApplyToError::new(
            format!("Method ->{} does not take any arguments", method_name).as_str(),
            input_path.to_vec(),
        ));
        None
    } else {
        let typeof_string = JSON::String(json_type_name(data).to_string().into());
        tail.apply_to_path(&typeof_string, vars, input_path, errors)
    }
}

fn map_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(args)) = method_args {
        if let Some(first_arg) = args.first() {
            if let JSON::Array(array) = data {
                let mut output = Vec::with_capacity(array.len());

                for (i, element) in array.iter().enumerate() {
                    let input_path = input_path.append(JSON::Number(i.into()));
                    if let Some(applied) =
                        first_arg.apply_to_path(element, vars, &input_path, errors)
                    {
                        if let Some(value) = tail.apply_to_path(&applied, vars, &input_path, errors)
                        {
                            output.push(value);
                            continue;
                        }
                    }
                    output.push(JSON::Null);
                }

                Some(JSON::Array(output))
            } else {
                first_arg.apply_to_path(data, vars, input_path, errors)
            }
        } else {
            errors.insert(ApplyToError::new(
                format!("Method ->{} requires one argument", method_name).as_str(),
                input_path.to_vec(),
            ));
            None
        }
    } else {
        errors.insert(ApplyToError::new(
            format!("Method ->{} requires one argument", method_name).as_str(),
            input_path.to_vec(),
        ));
        None
    }
}

fn eq_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(args)) = method_args {
        if args.len() == 1 {
            let matches = if let Some(value) = args[0].apply_to_path(data, vars, input_path, errors)
            {
                data == &value
            } else {
                false
            };
            return tail.apply_to_path(&JSON::Bool(matches), vars, input_path, errors);
        }
    }
    errors.insert(ApplyToError::new(
        format!("Method ->{} requires exactly one argument", method_name).as_str(),
        input_path.to_vec(),
    ));
    None
}

fn match_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    // Takes any number of pairs [key, value], and returns value for the first
    // key that equals the data. If none of the pairs match, returns None. A
    // single-element unconditional [value] may appear at the end.
    if let Some(MethodArgs(args)) = method_args {
        for pair in args {
            if let JSLiteral::Array(pair) = pair {
                if pair.len() == 1 {
                    return pair[0]
                        .apply_to_path(data, vars, input_path, errors)
                        .and_then(|value| tail.apply_to_path(&value, vars, input_path, errors));
                }

                if pair.len() == 2 {
                    if let Some(candidate) = pair[0].apply_to_path(data, vars, input_path, errors) {
                        if candidate == *data {
                            return pair[1]
                                .apply_to_path(data, vars, input_path, errors)
                                .and_then(|value| {
                                    tail.apply_to_path(&value, vars, input_path, errors)
                                });
                        }
                    };
                }
            }
        }
    }
    errors.insert(ApplyToError::new(
        format!(
            "Method ->{} did not match any [candidate, value] pair",
            method_name
        )
        .as_str(),
        input_path.to_vec(),
    ));
    None
}

// Like ->match, but expects the first element of each pair
// to evaluate to a boolean, returning the second element of
// the first pair whose first element is true. This makes
// providing a final catch-all case easy, since the last
// pair can be [true, <default>].
fn match_if_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(args)) = method_args {
        for pair in args {
            if let JSLiteral::Array(pair) = pair {
                if pair.len() == 2 {
                    if let Some(JSON::Bool(true)) =
                        pair[0].apply_to_path(data, vars, input_path, errors)
                    {
                        return pair[1]
                            .apply_to_path(data, vars, input_path, errors)
                            .and_then(|value| {
                                tail.apply_to_path(&value, vars, input_path, errors)
                            });
                    };
                }
            }
        }
    }
    errors.insert(ApplyToError::new(
        format!(
            "Method ->{} did not match any [condition, value] pair",
            method_name
        )
        .as_str(),
        input_path.to_vec(),
    ));
    None
}

fn arithmetic_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    op: impl Fn(&Number, &Number) -> Option<Number>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(args)) = method_args {
        if let JSON::Number(result) = data {
            let mut result = result.clone();
            for arg in args {
                let value_opt = arg.apply_to_path(data, vars, input_path, errors);
                if let Some(JSON::Number(n)) = value_opt {
                    if let Some(new_result) = op(&result, &n) {
                        result = new_result;
                    } else {
                        errors.insert(ApplyToError::new(
                            format!("Method ->{} failed on argument {}", method_name, n).as_str(),
                            input_path.to_vec(),
                        ));
                        return None;
                    }
                } else {
                    errors.insert(ApplyToError::new(
                        format!("Method ->{} requires numeric arguments", method_name).as_str(),
                        input_path.to_vec(),
                    ));
                    return None;
                }
            }
            Some(JSON::Number(result))
        } else {
            errors.insert(ApplyToError::new(
                format!("Method ->{} requires numeric arguments", method_name).as_str(),
                input_path.to_vec(),
            ));
            None
        }
    } else {
        errors.insert(ApplyToError::new(
            format!("Method ->{} requires at least one argument", method_name).as_str(),
            input_path.to_vec(),
        ));
        None
    }
}

macro_rules! infix_math_op {
    ($name:ident, $op:tt) => {
        fn $name(a: &Number, b: &Number) -> Option<Number> {
            if a.is_f64() || b.is_f64() {
                Number::from_f64(a.as_f64().unwrap() $op b.as_f64().unwrap())
            } else if let (Some(a_i64), Some(b_i64)) = (a.as_i64(), b.as_i64()) {
                Some(Number::from(a_i64 $op b_i64))
            } else {
                None
            }
        }
    };
}
infix_math_op!(add_op, +);
infix_math_op!(sub_op, -);
infix_math_op!(mul_op, *);
infix_math_op!(div_op, /);
infix_math_op!(rem_op, %);

macro_rules! infix_math_method {
    ($name:ident, $op:ident) => {
        fn $name(
            method_name: &str,
            method_args: &Option<MethodArgs>,
            data: &JSON,
            vars: &VarsWithPathsMap,
            input_path: &InputPath<JSON>,
            tail: &PathList,
            errors: &mut IndexSet<ApplyToError>,
        ) -> Option<JSON> {
            if let Some(result) = arithmetic_method(
                method_name,
                method_args,
                &$op,
                data,
                vars,
                input_path,
                errors,
            ) {
                tail.apply_to_path(&result, vars, input_path, errors)
            } else {
                None
            }
        }
    };
}
infix_math_method!(add_method, add_op);
infix_math_method!(sub_method, sub_op);
infix_math_method!(mul_method, mul_op);
infix_math_method!(div_method, div_op);
infix_math_method!(mod_method, rem_op);

fn first_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(_)) = method_args {
        errors.insert(ApplyToError::new(
            format!("Method ->{} does not take any arguments", method_name).as_str(),
            input_path.to_vec(),
        ));
        return None;
    }

    if let JSON::Array(array) = data {
        array
            .first()
            .and_then(|first| tail.apply_to_path(first, vars, input_path, errors))
    } else {
        tail.apply_to_path(data, vars, input_path, errors)
    }
}

fn last_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(_)) = method_args {
        errors.insert(ApplyToError::new(
            format!("Method ->{} does not take any arguments", method_name).as_str(),
            input_path.to_vec(),
        ));
        return None;
    }

    if let JSON::Array(array) = data {
        array
            .last()
            .and_then(|last| tail.apply_to_path(last, vars, input_path, errors))
    } else {
        tail.apply_to_path(data, vars, input_path, errors)
    }
}

fn slice_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    let length = if let JSON::Array(array) = data {
        array.len() as i64
    } else if let JSON::String(s) = data {
        s.as_str().len() as i64
    } else {
        errors.insert(ApplyToError::new(
            format!("Method ->{} requires an array or string input", method_name).as_str(),
            input_path.to_vec(),
        ));
        return None;
    };

    if let Some(MethodArgs(args)) = method_args {
        let start = args
            .first()
            .and_then(|arg| arg.apply_to_path(data, vars, input_path, errors))
            .and_then(|n| n.as_i64())
            .unwrap_or(0)
            .max(0)
            .min(length) as usize;
        let end = args
            .get(1)
            .and_then(|arg| arg.apply_to_path(data, vars, input_path, errors))
            .and_then(|n| n.as_i64())
            .unwrap_or(length)
            .max(0)
            .min(length) as usize;

        let array = match data {
            JSON::Array(array) => {
                if end - start > 0 {
                    JSON::Array(
                        array
                            .iter()
                            .skip(start)
                            .take(end - start)
                            .cloned()
                            .collect(),
                    )
                } else {
                    JSON::Array(vec![])
                }
            }
            JSON::String(s) => {
                if end - start > 0 {
                    JSON::String(s.as_str()[start..end].to_string().into())
                } else {
                    JSON::String("".to_string().into())
                }
            }
            _ => unreachable!(),
        };

        tail.apply_to_path(&array, vars, input_path, errors)
    } else {
        Some(data.clone())
    }
}

fn not_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if method_args.is_some() {
        errors.insert(ApplyToError::new(
            format!("Method ->{} does not take any arguments", method_name).as_str(),
            input_path.to_vec(),
        ));
        None
    } else {
        tail.apply_to_path(&JSON::Bool(!is_truthy(data)), vars, input_path, errors)
    }
}

fn is_truthy(data: &JSON) -> bool {
    match data {
        JSON::Bool(b) => *b,
        JSON::Number(n) => n.as_f64().map_or(false, |n| n != 0.0),
        JSON::Null => false,
        JSON::String(s) => !s.as_str().is_empty(),
        JSON::Object(_) | JSON::Array(_) => true,
    }
}

fn or_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(args)) = method_args {
        let mut result = is_truthy(data);
        for arg in args {
            if result {
                break;
            }
            result = arg
                .apply_to_path(data, vars, input_path, errors)
                .map(|value| is_truthy(&value))
                .unwrap_or(false);
        }
        tail.apply_to_path(&JSON::Bool(result), vars, input_path, errors)
    } else {
        errors.insert(ApplyToError::new(
            format!("Method ->{} requires arguments", method_name).as_str(),
            input_path.to_vec(),
        ));
        None
    }
}

fn and_method(
    method_name: &str,
    method_args: &Option<MethodArgs>,
    data: &JSON,
    vars: &VarsWithPathsMap,
    input_path: &InputPath<JSON>,
    tail: &PathList,
    errors: &mut IndexSet<ApplyToError>,
) -> Option<JSON> {
    if let Some(MethodArgs(args)) = method_args {
        let mut result = is_truthy(data);
        for arg in args {
            if !result {
                break;
            }
            result = arg
                .apply_to_path(data, vars, input_path, errors)
                .map(|value| is_truthy(&value))
                .unwrap_or(false);
        }
        tail.apply_to_path(&JSON::Bool(result), vars, input_path, errors)
    } else {
        errors.insert(ApplyToError::new(
            format!("Method ->{} requires arguments", method_name).as_str(),
            input_path.to_vec(),
        ));
        None
    }
}

#[cfg(test)]
mod tests {
    use serde_json_bytes::json;

    use super::*;
    use crate::selection;

    #[test]
    fn test_echo_method() {
        assert_eq!(
            selection!("$->echo('oyez')").apply_to(&json!(null)),
            (Some(json!("oyez")), vec![]),
        );

        assert_eq!(
            selection!("$->echo('oyez')").apply_to(&json!([1, 2, 3])),
            (Some(json!("oyez")), vec![]),
        );

        assert_eq!(
            selection!("$->echo([1, 2, 3]) { id: $ }").apply_to(&json!(null)),
            (Some(json!([{ "id": 1 }, { "id": 2 }, { "id": 3 }])), vec![]),
        );

        assert_eq!(
            selection!("$->echo([1, 2, 3])->last { id: $ }").apply_to(&json!(null)),
            (Some(json!({ "id": 3 })), vec![]),
        );

        assert_eq!(
            selection!("$->echo([1.1, 0.2, -3.3]) { id: $ }").apply_to(&json!(null)),
            (
                Some(json!([{ "id": 1.1 }, { "id": 0.2 }, { "id": -3.3 }])),
                vec![]
            ),
        );

        assert_eq!(
            selection!("$.nested.value->echo(['before', @, 'after'])").apply_to(&json!({
                "nested": {
                    "value": 123,
                },
            })),
            (Some(json!(["before", 123, "after"])), vec![]),
        );

        assert_eq!(
            selection!("$.nested.value->echo(['before', $, 'after'])").apply_to(&json!({
                "nested": {
                    "value": 123,
                },
            })),
            (
                Some(json!(["before", {
                "nested": {
                    "value": 123,
                },
            }, "after"])),
                vec![]
            ),
        );

        assert_eq!(
            selection!("data->echo(@.results->last)").apply_to(&json!({
                "data": {
                    "results": [1, 2, 3],
                },
            })),
            (Some(json!(3)), vec![]),
        );

        assert_eq!(
            selection!("results->echo(@->first)").apply_to(&json!({
                "results": [
                    [1, 2, 3],
                    "ignored",
                ],
            })),
            (Some(json!([1, 2, 3])), vec![]),
        );

        assert_eq!(
            selection!("results->echo(@->first)->last").apply_to(&json!({
                "results": [
                    [1, 2, 3],
                    "ignored",
                ],
            })),
            (Some(json!(3)), vec![]),
        );
    }

    #[test]
    fn test_typeof_method() {
        fn check(selection: &str, data: &JSON, expected_type: &str) {
            assert_eq!(
                selection!(selection).apply_to(data),
                (Some(json!(expected_type)), vec![]),
            );
        }

        check("$->typeof", &json!(null), "null");
        check("$->typeof", &json!(true), "boolean");
        check("@->typeof", &json!(false), "boolean");
        check("$->typeof", &json!(123), "number");
        check("$->typeof", &json!(123.45), "number");
        check("$->typeof", &json!("hello"), "string");
        check("$->typeof", &json!([1, 2, 3]), "array");
        check("$->typeof", &json!({ "key": "value" }), "object");
    }

    #[test]
    fn test_map_method() {
        assert_eq!(
            selection!("$->map(@->add(10))").apply_to(&json!([1, 2, 3])),
            (Some(json!(vec![11, 12, 13])), vec![]),
        );

        assert_eq!(
            selection!("messages->map(@.role)").apply_to(&json!({
                "messages": [
                    { "role": "admin" },
                    { "role": "user" },
                    { "role": "guest" },
                ],
            })),
            (Some(json!(["admin", "user", "guest"])), vec![]),
        );

        assert_eq!(
            selection!("messages->map(@.roles)").apply_to(&json!({
                "messages": [
                    { "roles": ["admin"] },
                    { "roles": ["user", "guest"] },
                ],
            })),
            (Some(json!([["admin"], ["user", "guest"]])), vec![]),
        );

        assert_eq!(
            selection!("values->map(@->typeof)").apply_to(&json!({
                "values": [1, 2.5, "hello", true, null, [], {}],
            })),
            (
                Some(json!([
                    "number", "number", "string", "boolean", "null", "array", "object"
                ])),
                vec![],
            ),
        );

        assert_eq!(
            selection!("singleValue->map(@->mul(10))").apply_to(&json!({
                "singleValue": 123,
            })),
            (Some(json!(1230)), vec![]),
        );
    }

    #[test]
    fn test_missing_method() {
        assert_eq!(
            selection!("nested.path->bogus").apply_to(&json!({
                "nested": {
                    "path": 123,
                },
            })),
            (
                None,
                vec![ApplyToError::from_json(&json!({
                    "message": "Method ->bogus not found",
                    "path": ["nested", "path"],
                }))],
            ),
        );
    }

    #[test]
    fn test_match_methods() {
        assert_eq!(
            selection!(
                r#"
                name
                __typename: kind->match(
                    ['dog', 'Canine'],
                    ['cat', 'Feline']
                )
                "#
            )
            .apply_to(&json!({
                "kind": "cat",
                "name": "Whiskers",
            })),
            (
                Some(json!({
                    "__typename": "Feline",
                    "name": "Whiskers",
                })),
                vec![],
            ),
        );

        assert_eq!(
            selection!(
                r#"
                name
                __typename: kind->match(
                    ['dog', 'Canine'],
                    ['cat', 'Feline'],
                    [@, 'Exotic']
                )
                "#
            )
            .apply_to(&json!({
                "kind": "axlotl",
                "name": "Gulpy",
            })),
            (
                Some(json!({
                    "__typename": "Exotic",
                    "name": "Gulpy",
                })),
                vec![],
            ),
        );

        assert_eq!(
            selection!(
                r#"
                name
                __typename: kind->match(
                    ['dog', 'Canine'],
                    ['cat', 'Feline'],
                    ['Exotic']
                )
                "#
            )
            .apply_to(&json!({
                "kind": "axlotl",
                "name": "Gulpy",
            })),
            (
                Some(json!({
                    "__typename": "Exotic",
                    "name": "Gulpy",
                })),
                vec![],
            ),
        );

        assert_eq!(
            selection!(
                r#"
                name
                __typename: kind->match(
                    ['dog', 'Canine'],
                    ['cat', 'Feline'],
                    ['Exotic']
                )
                "#
            )
            .apply_to(&json!({
                "kind": "dog",
                "name": "Laika",
            })),
            (
                Some(json!({
                    "__typename": "Canine",
                    "name": "Laika",
                })),
                vec![],
            ),
        );

        assert_eq!(
            selection!(
                r#"
                num: value->matchIf(
                    [@->typeof->eq('number'), @],
                    [true, 'not a number']
                )
                "#
            )
            .apply_to(&json!({ "value": 123 })),
            (
                Some(json!({
                    "num": 123,
                })),
                vec![],
            ),
        );

        assert_eq!(
            selection!(
                r#"
                num: value->matchIf(
                    [@->typeof->eq('number'), @],
                    [true, 'not a number']
                )
                "#
            )
            .apply_to(&json!({ "value": true })),
            (
                Some(json!({
                    "num": "not a number",
                })),
                vec![],
            ),
        );

        assert_eq!(
            selection!(
                r#"
                result->matchIf(
                    [@->typeof->eq('boolean'), @],
                    [true, 'not boolean']
                )
                "#
            )
            .apply_to(&json!({
                "result": true,
            })),
            (Some(json!(true)), vec![]),
        );

        assert_eq!(
            selection!(
                r#"
                result->match_if(
                    [@->typeof->eq('boolean'), @],
                    [true, 'not boolean']
                )
                "#
            )
            .apply_to(&json!({
                "result": 321,
            })),
            (Some(json!("not boolean")), vec![]),
        );
    }

    fn test_arithmetic_methods() {
        assert_eq!(
            selection!("$->add(1)").apply_to(&json!(2)),
            (Some(json!(3)), vec![]),
        );
        assert_eq!(
            selection!("$->add(1.5)").apply_to(&json!(2)),
            (Some(json!(3.5)), vec![]),
        );
        assert_eq!(
            selection!("$->add(1)").apply_to(&json!(2.5)),
            (Some(json!(3.5)), vec![]),
        );
        assert_eq!(
            selection!("$->add(1, 2, 3, 5, 8)").apply_to(&json!(1)),
            (Some(json!(20)), vec![]),
        );

        assert_eq!(
            selection!("$->sub(1)").apply_to(&json!(2)),
            (Some(json!(1)), vec![]),
        );
        assert_eq!(
            selection!("$->sub(1.5)").apply_to(&json!(2)),
            (Some(json!(0.5)), vec![]),
        );
        assert_eq!(
            selection!("$->sub(10)").apply_to(&json!(2.5)),
            (Some(json!(-7.5)), vec![]),
        );
        assert_eq!(
            selection!("$->sub(10, 2.5)").apply_to(&json!(2.5)),
            (Some(json!(-10.0)), vec![]),
        );

        assert_eq!(
            selection!("$->mul(2)").apply_to(&json!(3)),
            (Some(json!(6)), vec![]),
        );
        assert_eq!(
            selection!("$->mul(2.5)").apply_to(&json!(3)),
            (Some(json!(7.5)), vec![]),
        );
        assert_eq!(
            selection!("$->mul(2)").apply_to(&json!(3.5)),
            (Some(json!(7.0)), vec![]),
        );
        assert_eq!(
            selection!("$->mul(-2.5)").apply_to(&json!(3.5)),
            (Some(json!(-8.75)), vec![]),
        );
        assert_eq!(
            selection!("$->mul(2, 3, 5, 7)").apply_to(&json!(10)),
            (Some(json!(2100)), vec![]),
        );

        assert_eq!(
            selection!("$->div(2)").apply_to(&json!(6)),
            (Some(json!(3)), vec![]),
        );
        assert_eq!(
            selection!("$->div(2.5)").apply_to(&json!(7.5)),
            (Some(json!(3.0)), vec![]),
        );
        assert_eq!(
            selection!("$->div(2)").apply_to(&json!(7)),
            (Some(json!(3)), vec![]),
        );
        assert_eq!(
            selection!("$->div(2.5)").apply_to(&json!(7)),
            (Some(json!(2.8)), vec![]),
        );
        assert_eq!(
            selection!("$->div(2, 3, 5, 7)").apply_to(&json!(2100)),
            (Some(json!(10)), vec![]),
        );

        assert_eq!(
            selection!("$->mod(2)").apply_to(&json!(6)),
            (Some(json!(0)), vec![]),
        );
        assert_eq!(
            selection!("$->mod(2.5)").apply_to(&json!(7.5)),
            (Some(json!(0.0)), vec![]),
        );
        assert_eq!(
            selection!("$->mod(2)").apply_to(&json!(7)),
            (Some(json!(1)), vec![]),
        );
        assert_eq!(
            selection!("$->mod(4)").apply_to(&json!(7)),
            (Some(json!(3)), vec![]),
        );
        assert_eq!(
            selection!("$->mod(2.5)").apply_to(&json!(7)),
            (Some(json!(2.0)), vec![]),
        );
        assert_eq!(
            selection!("$->mod(2, 3, 5, 7)").apply_to(&json!(2100)),
            (Some(json!(0)), vec![]),
        );
    }

    #[test]
    fn test_array_methods() {
        assert_eq!(
            selection!("$->first").apply_to(&json!([1, 2, 3])),
            (Some(json!(1)), vec![]),
        );
        assert_eq!(selection!("$->first").apply_to(&json!([])), (None, vec![]),);
        assert_eq!(
            selection!("$->first").apply_to(&json!("hello")),
            (Some(json!("hello")), vec![]),
        );

        assert_eq!(
            selection!("$->last").apply_to(&json!([1, 2, 3])),
            (Some(json!(3)), vec![]),
        );
        assert_eq!(selection!("$->last").apply_to(&json!([])), (None, vec![]),);
        assert_eq!(
            selection!("$->last").apply_to(&json!("hello")),
            (Some(json!("hello")), vec![]),
        );

        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!([1, 2, 3, 4, 5])),
            (Some(json!([2, 3])), vec![]),
        );
        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!([1, 2])),
            (Some(json!([2])), vec![]),
        );
        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!([1])),
            (Some(json!([])), vec![]),
        );
        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!([])),
            (Some(json!([])), vec![]),
        );
        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!("hello")),
            (Some(json!("el")), vec![]),
        );
        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!("he")),
            (Some(json!("e")), vec![]),
        );
        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!("h")),
            (Some(json!("")), vec![]),
        );
        assert_eq!(
            selection!("$->slice(1, 3)").apply_to(&json!("")),
            (Some(json!("")), vec![]),
        );
    }

    #[test]
    fn test_logical_methods() {
        assert_eq!(
            selection!("$->map(@->not)").apply_to(&json!([
                true,
                false,
                0,
                1,
                -123,
                null,
                "hello",
                {},
                [],
            ])),
            (
                Some(json!([
                    false, true, true, false, false, true, false, false, false,
                ])),
                vec![],
            ),
        );

        assert_eq!(
            selection!("$->map(@->not->not)").apply_to(&json!([
                true,
                false,
                0,
                1,
                -123,
                null,
                "hello",
                {},
                [],
            ])),
            (
                Some(json!([
                    true, false, false, true, true, false, true, true, true,
                ])),
                vec![],
            ),
        );

        assert_eq!(
            selection!("$.a->and($.b, $.c)").apply_to(&json!({
                "a": true,
                "b": null,
                "c": true,
            })),
            (Some(json!(false)), vec![]),
        );
        assert_eq!(
            selection!("$.b->and($.c, $.a)").apply_to(&json!({
                "a": "hello",
                "b": true,
                "c": 123,
            })),
            (Some(json!(true)), vec![]),
        );
        assert_eq!(
            selection!("$.both->and($.and)").apply_to(&json!({
                "both": true,
                "and": true,
            })),
            (Some(json!(true)), vec![]),
        );
        assert_eq!(
            selection!("data.x->and($.data.y)").apply_to(&json!({
                "data": {
                    "x": true,
                    "y": false,
                },
            })),
            (Some(json!(false)), vec![]),
        );

        assert_eq!(
            selection!("$.a->or($.b, $.c)").apply_to(&json!({
                "a": true,
                "b": null,
                "c": true,
            })),
            (Some(json!(true)), vec![]),
        );
        assert_eq!(
            selection!("$.b->or($.a, $.c)").apply_to(&json!({
                "a": false,
                "b": null,
                "c": 0,
            })),
            (Some(json!(false)), vec![]),
        );
        assert_eq!(
            selection!("$.both->or($.and)").apply_to(&json!({
                "both": true,
                "and": false,
            })),
            (Some(json!(true)), vec![]),
        );
        assert_eq!(
            selection!("data.x->or($.data.y)").apply_to(&json!({
                "data": {
                    "x": false,
                    "y": false,
                },
            })),
            (Some(json!(false)), vec![]),
        );
    }
}
