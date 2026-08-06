//! Order-preserving, scene-bound simplification for imported Mandelbulber formulas.
//!
//! The pass intentionally reasons only about structural integer state. It never
//! folds floating-point expressions or changes the order of surviving formula
//! operations. Unknown constructs are emitted unchanged.

use super::compiler::{Token, TokenKind, lex};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StructuralValue {
    Bool(bool),
    Integer(i64),
    Symbol(String),
    IntegerRange { first: i64, last: i64 },
}

impl StructuralValue {
    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Integer(value) => Some(*value != 0),
            _ => None,
        }
    }

    fn singleton_integer(&self) -> Option<i64> {
        match self {
            Self::Bool(value) => Some(i64::from(*value)),
            Self::Integer(value) => Some(*value),
            Self::IntegerRange { first, last } if first == last => Some(*first),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FormulaSimplificationStats {
    pub conditions_removed: usize,
    pub common_initializers_removed: usize,
    pub dead_declarations_removed: usize,
    pub loops_scalarized: usize,
    pub switches_removed: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct SimplifiedFormulaBody {
    pub source: String,
    pub stats: FormulaSimplificationStats,
}

pub(crate) fn simplify_formula_body(
    source: &str,
    structural_constants: &BTreeMap<String, StructuralValue>,
    iteration_range: Option<(i64, i64)>,
    scalarize_fixed_loops: bool,
    eliminate_dead_locals: bool,
    eliminate_common_initializers: bool,
) -> Result<SimplifiedFormulaBody> {
    let tokens = lex(source)?;
    let mut constants = structural_constants.clone();
    if let Some((first, last)) = iteration_range {
        constants.insert(
            "aux.i".into(),
            StructuralValue::IntegerRange { first, last },
        );
    }
    collect_local_structural_constants(&tokens, &mut constants);
    let mut stats = FormulaSimplificationStats::default();
    let mut simplified = simplify_range(
        &tokens,
        0,
        tokens.len(),
        &constants,
        scalarize_fixed_loops,
        &mut stats,
    )?;
    if eliminate_common_initializers {
        let (without_common_initializers, removed) =
            eliminate_adjacent_common_initializers(&simplified)?;
        simplified = without_common_initializers;
        stats.common_initializers_removed = removed;
    }
    if eliminate_dead_locals {
        let (without_dead_locals, removed) = eliminate_dead_pure_declarations(&simplified)?;
        simplified = without_dead_locals;
        stats.dead_declarations_removed = removed;
    }
    let rendered = if stats == FormulaSimplificationStats::default() {
        // Avoid perturbing compiler code generation when the proof pass found
        // nothing to change. This is also the zero-rewrite bisection baseline.
        source.to_owned()
    } else {
        render_tokens(&simplified)
    };
    Ok(SimplifiedFormulaBody {
        source: rendered,
        stats,
    })
}

fn eliminate_adjacent_common_initializers(tokens: &[Token]) -> Result<(Vec<Token>, usize)> {
    let mut current = tokens.to_vec();
    let mut removed = 0usize;
    loop {
        let mut previous = None::<(String, Vec<String>, String, usize)>;
        let mut replacement = None::<(usize, usize, String, String)>;
        let mut index = 0usize;
        while index + 3 < current.len() {
            let statement_start =
                index == 0 || matches!(current[index - 1].text.as_str(), ";" | "{" | "}");
            if !statement_start || !is_local_value_type(&current[index].text) {
                previous = None;
                index += 1;
                continue;
            }
            let name_index = index + 1;
            if current[name_index].kind != TokenKind::Identifier
                || current.get(name_index + 1).map(|token| token.text.as_str()) != Some("=")
            {
                previous = None;
                index += 1;
                continue;
            }
            let end = statement_end(&current, index, current.len())?;
            let initializer = &current[name_index + 2..end - 1];
            if !declaration_initializer_is_pure(&current[name_index + 1..end - 1]) {
                previous = None;
                index = end;
                continue;
            }
            let initializer_key = initializer
                .iter()
                .map(|token| token.text.clone())
                .collect::<Vec<_>>();
            let name = current[name_index].text.clone();
            if let Some((previous_type, previous_initializer, previous_name, previous_end)) =
                previous.as_ref()
                && previous_type == &current[index].text
                && previous_initializer == &initializer_key
                && !variable_is_written(&current[*previous_end..], previous_name)
                && !variable_is_written(&current[end..], &name)
            {
                replacement = Some((index, end, name, previous_name.clone()));
                break;
            }
            previous = Some((current[index].text.clone(), initializer_key, name, end));
            index = end;
        }
        let Some((start, end, from, to)) = replacement else {
            break;
        };
        current.drain(start..end);
        for token in &mut current[start..] {
            if token.kind == TokenKind::Identifier && token.text == from {
                token.text = to.clone();
            }
        }
        removed += 1;
    }
    Ok((current, removed))
}

fn variable_is_written(tokens: &[Token], name: &str) -> bool {
    const WRITES: [&str; 9] = ["=", "+=", "-=", "*=", "/=", "%=", "++", "--", "&="];
    tokens.iter().enumerate().any(|(index, token)| {
        token.kind == TokenKind::Identifier
            && token.text == name
            && (tokens
                .get(index + 1)
                .is_some_and(|token| WRITES.contains(&token.text.as_str()))
                || index
                    .checked_sub(1)
                    .and_then(|index| tokens.get(index))
                    .is_some_and(|token| matches!(token.text.as_str(), "++" | "--")))
    })
}

fn eliminate_dead_pure_declarations(tokens: &[Token]) -> Result<(Vec<Token>, usize)> {
    let mut current = tokens.to_vec();
    let mut removed = 0usize;
    loop {
        let mut removal = None;
        let mut index = 0usize;
        while index + 1 < current.len() {
            let statement_start =
                index == 0 || matches!(current[index - 1].text.as_str(), ";" | "{" | "}");
            if !statement_start || !is_local_value_type(&current[index].text) {
                index += 1;
                continue;
            }
            let name_index = index + 1;
            if current[name_index].kind != TokenKind::Identifier {
                index += 1;
                continue;
            }
            let end = statement_end(&current, index, current.len())?;
            let name = &current[name_index].text;
            let uses = current
                .iter()
                .filter(|token| token.kind == TokenKind::Identifier && token.text == *name)
                .count();
            if uses == 1 && declaration_initializer_is_pure(&current[name_index + 1..end - 1]) {
                removal = Some((index, end));
                break;
            }
            index = end;
        }
        let Some((start, end)) = removal else {
            break;
        };
        current.drain(start..end);
        removed += 1;
    }
    Ok((current, removed))
}

fn is_local_value_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "int"
            | "uint"
            | "float"
            | "float2"
            | "float3"
            | "float4"
            | "REAL"
            | "REAL2"
            | "REAL3"
            | "REAL4"
            | "matrix33"
    ) || name.starts_with("enum")
}

fn declaration_initializer_is_pure(tokens: &[Token]) -> bool {
    let Some(equals) = tokens.iter().position(|token| token.text == "=") else {
        return true;
    };
    let initializer = &tokens[equals + 1..];
    !initializer.iter().any(|token| {
        matches!(
            token.text.as_str(),
            "++" | "--" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^="
        )
    }) && !initializer
        .windows(2)
        .any(|tokens| tokens[0].kind == TokenKind::Identifier && tokens[1].text == "(")
}

fn simplify_range(
    tokens: &[Token],
    start: usize,
    end: usize,
    constants: &BTreeMap<String, StructuralValue>,
    scalarize_fixed_loops: bool,
    stats: &mut FormulaSimplificationStats,
) -> Result<Vec<Token>> {
    let mut output = Vec::new();
    let mut index = start;
    while index < end {
        if tokens[index].text == "switch"
            && tokens.get(index + 1).is_some_and(|token| token.text == "(")
        {
            let condition_end = matching_token(tokens, index + 1, "(", ")")?;
            let body_start = condition_end + 1;
            if tokens
                .get(body_start)
                .is_some_and(|token| token.text == "{")
            {
                let body_end = matching_token(tokens, body_start, "{", "}")?;
                if let Some(value) =
                    evaluate_expression(&tokens[index + 2..condition_end], constants)
                    && let Some(selected) =
                        select_switch_body(tokens, body_start + 1, body_end, &value, constants)
                {
                    stats.switches_removed += 1;
                    output.push(tokens[body_start].clone());
                    output.extend(simplify_range(
                        &selected,
                        0,
                        selected.len(),
                        constants,
                        scalarize_fixed_loops,
                        stats,
                    )?);
                    output.push(tokens[body_end].clone());
                    index = body_end + 1;
                    continue;
                }
            }
        }

        if tokens[index].text == "if"
            && tokens.get(index + 1).is_some_and(|token| token.text == "(")
        {
            let condition_end = matching_token(tokens, index + 1, "(", ")")?;
            let then_start = condition_end + 1;
            let then_end = statement_end(tokens, then_start, end)?;
            let (else_start, else_end) = if tokens
                .get(then_end)
                .is_some_and(|token| token.text == "else")
            {
                let body_start = then_end + 1;
                (
                    Some(body_start),
                    Some(statement_end(tokens, body_start, end)?),
                )
            } else {
                (None, None)
            };
            let condition = evaluate_expression(&tokens[index + 2..condition_end], constants)
                .and_then(|value| value.as_bool());
            if let Some(condition) = condition {
                stats.conditions_removed += 1;
                if condition {
                    output.extend(simplify_statement(
                        tokens,
                        then_start,
                        then_end,
                        constants,
                        scalarize_fixed_loops,
                        stats,
                    )?);
                } else if let (Some(else_start), Some(else_end)) = (else_start, else_end) {
                    output.extend(simplify_statement(
                        tokens,
                        else_start,
                        else_end,
                        constants,
                        scalarize_fixed_loops,
                        stats,
                    )?);
                }
                index = else_end.unwrap_or(then_end);
                continue;
            }

            output.extend_from_slice(&tokens[index..=condition_end]);
            output.extend(simplify_statement(
                tokens,
                then_start,
                then_end,
                constants,
                scalarize_fixed_loops,
                stats,
            )?);
            if let (Some(else_start), Some(else_end)) = (else_start, else_end) {
                output.push(tokens[then_end].clone());
                output.extend(simplify_statement(
                    tokens,
                    else_start,
                    else_end,
                    constants,
                    scalarize_fixed_loops,
                    stats,
                )?);
                index = else_end;
            } else {
                index = then_end;
            }
            continue;
        }

        if scalarize_fixed_loops
            && tokens[index].text == "for"
            && tokens.get(index + 1).is_some_and(|token| token.text == "(")
        {
            let header_end = matching_token(tokens, index + 1, "(", ")")?;
            let body_start = header_end + 1;
            let body_end = statement_end(tokens, body_start, end)?;
            if let Some((variable, first, last)) = fixed_loop_range(&tokens[index + 2..header_end])
                && last >= first
                && last - first <= 8
                && !loop_has_unsafe_transfer(&tokens[body_start..body_end])
            {
                stats.loops_scalarized += 1;
                for value in first..last {
                    let substituted =
                        substitute_identifier(&tokens[body_start..body_end], &variable, value);
                    output.extend(simplify_statement(
                        &substituted,
                        0,
                        substituted.len(),
                        constants,
                        scalarize_fixed_loops,
                        stats,
                    )?);
                }
                index = body_end;
                continue;
            }
        }

        if tokens[index].text == "{" {
            let close = matching_token(tokens, index, "{", "}")?;
            output.push(tokens[index].clone());
            output.extend(simplify_range(
                tokens,
                index + 1,
                close,
                constants,
                scalarize_fixed_loops,
                stats,
            )?);
            output.push(tokens[close].clone());
            index = close + 1;
            continue;
        }

        output.push(tokens[index].clone());
        index += 1;
    }
    Ok(output)
}

fn select_switch_body(
    tokens: &[Token],
    start: usize,
    end: usize,
    selected: &StructuralValue,
    constants: &BTreeMap<String, StructuralValue>,
) -> Option<Vec<Token>> {
    let mut labels = Vec::<(Option<StructuralValue>, usize)>::new();
    let mut depth = 0usize;
    let mut index = start;
    while index < end {
        match tokens[index].text.as_str() {
            "{" => depth += 1,
            "}" => depth = depth.saturating_sub(1),
            "case" if depth == 0 => {
                let colon = (index + 1..end).find(|candidate| tokens[*candidate].text == ":")?;
                labels.push((
                    evaluate_expression(&tokens[index + 1..colon], constants),
                    colon + 1,
                ));
                index = colon;
            }
            "default"
                if depth == 0 && tokens.get(index + 1).is_some_and(|token| token.text == ":") =>
            {
                labels.push((None, index + 2));
                index += 1;
            }
            _ => {}
        }
        index += 1;
    }
    let selected_start = labels
        .iter()
        .find_map(|(value, start)| {
            value
                .as_ref()
                .and_then(|value| values_equal(value, selected))
                .is_some_and(|equal| equal)
                .then_some(*start)
        })
        .or_else(|| {
            labels
                .iter()
                .find_map(|(value, start)| value.is_none().then_some(*start))
        })?;

    let mut output = Vec::new();
    depth = 0;
    index = selected_start;
    while index < end {
        match tokens[index].text.as_str() {
            "{" => {
                depth += 1;
                output.push(tokens[index].clone());
            }
            "}" => {
                depth = depth.saturating_sub(1);
                output.push(tokens[index].clone());
            }
            "break"
                if depth == 0 && tokens.get(index + 1).is_some_and(|token| token.text == ";") =>
            {
                break;
            }
            "case" | "default" if depth == 0 => {
                let colon = (index..end).find(|candidate| tokens[*candidate].text == ":")?;
                index = colon;
            }
            _ => output.push(tokens[index].clone()),
        }
        index += 1;
    }
    Some(output)
}

fn loop_has_unsafe_transfer(tokens: &[Token]) -> bool {
    if tokens.iter().any(|token| token.text == "continue") {
        return true;
    }
    let mut switch_ranges = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.text != "switch" || !tokens.get(index + 1).is_some_and(|token| token.text == "(") {
            continue;
        }
        let Ok(header_end) = matching_token(tokens, index + 1, "(", ")") else {
            continue;
        };
        if !tokens
            .get(header_end + 1)
            .is_some_and(|token| token.text == "{")
        {
            continue;
        }
        if let Ok(body_end) = matching_token(tokens, header_end + 1, "{", "}") {
            switch_ranges.push((header_end + 1, body_end));
        }
    }
    tokens.iter().enumerate().any(|(index, token)| {
        token.text == "break"
            && !switch_ranges
                .iter()
                .any(|(start, end)| *start < index && index < *end)
    })
}

fn simplify_statement(
    tokens: &[Token],
    start: usize,
    end: usize,
    constants: &BTreeMap<String, StructuralValue>,
    scalarize_fixed_loops: bool,
    stats: &mut FormulaSimplificationStats,
) -> Result<Vec<Token>> {
    if start < end && tokens[start].text == "{" {
        ensure!(tokens[end - 1].text == "}", "statement block is unbalanced");
        let mut output = vec![tokens[start].clone()];
        output.extend(simplify_range(
            tokens,
            start + 1,
            end - 1,
            constants,
            scalarize_fixed_loops,
            stats,
        )?);
        output.push(tokens[end - 1].clone());
        Ok(output)
    } else {
        simplify_range(tokens, start, end, constants, scalarize_fixed_loops, stats)
    }
}

fn statement_end(tokens: &[Token], start: usize, limit: usize) -> Result<usize> {
    ensure!(start < limit, "control statement has no body");
    if tokens[start].text == "{" {
        return Ok(matching_token(tokens, start, "{", "}")? + 1);
    }
    if tokens[start].text == "if" {
        ensure!(tokens.get(start + 1).is_some_and(|token| token.text == "("));
        let condition_end = matching_token(tokens, start + 1, "(", ")")?;
        let then_end = statement_end(tokens, condition_end + 1, limit)?;
        if tokens
            .get(then_end)
            .is_some_and(|token| token.text == "else")
        {
            return statement_end(tokens, then_end + 1, limit);
        }
        return Ok(then_end);
    }
    if matches!(tokens[start].text.as_str(), "for" | "while" | "switch")
        && tokens.get(start + 1).is_some_and(|token| token.text == "(")
    {
        let header_end = matching_token(tokens, start + 1, "(", ")")?;
        return statement_end(tokens, header_end + 1, limit);
    }
    let mut round = 0usize;
    let mut square = 0usize;
    for (index, token) in tokens.iter().enumerate().take(limit).skip(start) {
        match token.text.as_str() {
            "(" => round += 1,
            ")" => round = round.saturating_sub(1),
            "[" => square += 1,
            "]" => square = square.saturating_sub(1),
            ";" if round == 0 && square == 0 => return Ok(index + 1),
            _ => {}
        }
    }
    ensure!(
        false,
        "statement beginning at token {start} has no terminator"
    );
    unreachable!()
}

fn matching_token(tokens: &[Token], open: usize, left: &str, right: &str) -> Result<usize> {
    ensure!(tokens.get(open).is_some_and(|token| token.text == left));
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token.text == left {
            depth += 1;
        } else if token.text == right {
            depth -= 1;
            if depth == 0 {
                return Ok(index);
            }
        }
    }
    ensure!(false, "unclosed {left} delimiter");
    unreachable!()
}

fn fixed_loop_range(tokens: &[Token]) -> Option<(String, i64, i64)> {
    let separators = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (token.text == ";").then_some(index))
        .collect::<Vec<_>>();
    if separators.len() != 2 {
        return None;
    }
    let init = &tokens[..separators[0]];
    let condition = &tokens[separators[0] + 1..separators[1]];
    let increment = &tokens[separators[1] + 1..];
    let equals = init.iter().position(|token| token.text == "=")?;
    let variable = init.get(equals.wrapping_sub(1))?.text.clone();
    let first = parse_integer_token(init.get(equals + 1)?)?;
    if condition.len() != 3 || condition[0].text != variable || condition[1].text != "<" {
        return None;
    }
    let last = parse_integer_token(&condition[2])?;
    let increments = matches!(
        increment.iter().map(|token| token.text.as_str()).collect::<Vec<_>>().as_slice(),
        [name, "++"] if *name == variable
    ) || matches!(
        increment.iter().map(|token| token.text.as_str()).collect::<Vec<_>>().as_slice(),
        ["++", name] if *name == variable
    );
    increments.then_some((variable, first, last))
}

fn substitute_identifier(tokens: &[Token], name: &str, value: i64) -> Vec<Token> {
    tokens
        .iter()
        .map(|token| {
            if token.kind == TokenKind::Identifier && token.text == name {
                let mut replacement = token.clone();
                replacement.kind = TokenKind::Number;
                replacement.text = value.to_string();
                replacement
            } else {
                token.clone()
            }
        })
        .collect()
}

fn collect_local_structural_constants(
    tokens: &[Token],
    constants: &mut BTreeMap<String, StructuralValue>,
) {
    let mut index = 0usize;
    while index + 6 < tokens.len() {
        if !(matches!(tokens[index].text.as_str(), "bool" | "int" | "uint")
            || tokens[index].text.starts_with("enum"))
            || tokens[index + 1].kind != TokenKind::Identifier
            || tokens[index + 2].text != "["
        {
            index += 1;
            continue;
        }
        let Some(square_end) = matching_token(tokens, index + 2, "[", "]").ok() else {
            index += 1;
            continue;
        };
        if tokens.get(square_end + 1).map(|token| token.text.as_str()) != Some("=")
            || tokens.get(square_end + 2).map(|token| token.text.as_str()) != Some("{")
        {
            index += 1;
            continue;
        }
        let Some(brace_end) = matching_token(tokens, square_end + 2, "{", "}").ok() else {
            index += 1;
            continue;
        };
        let mut item_start = square_end + 3;
        let mut item_index = 0usize;
        let mut nested = 0usize;
        for cursor in item_start..=brace_end {
            let boundary = cursor == brace_end || (tokens[cursor].text == "," && nested == 0);
            if boundary {
                if let Some(value) = evaluate_expression(&tokens[item_start..cursor], constants) {
                    constants.insert(format!("{}[{item_index}]", tokens[index + 1].text), value);
                }
                item_index += 1;
                item_start = cursor + 1;
                continue;
            }
            match tokens[cursor].text.as_str() {
                "(" | "[" | "{" => nested += 1,
                ")" | "]" | "}" => nested = nested.saturating_sub(1),
                _ => {}
            }
        }
        index = brace_end + 1;
    }
}

fn render_tokens(tokens: &[Token]) -> String {
    let mut output = String::new();
    let mut previous = None::<&Token>;
    for token in tokens {
        if token.kind == TokenKind::Preprocessor {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&token.text);
            output.push('\n');
            previous = None;
        } else {
            if previous.is_some_and(|previous| token_words_need_space(previous, token))
                && !output.ends_with([' ', '\n'])
            {
                output.push(' ');
            }
            output.push_str(&token.text);
            if matches!(token.text.as_str(), ";" | "{" | "}") {
                output.push('\n');
                previous = None;
            } else {
                previous = Some(token);
            }
        }
    }
    output
}

fn token_words_need_space(left: &Token, right: &Token) -> bool {
    let word = |token: &Token| {
        matches!(
            token.kind,
            TokenKind::Identifier | TokenKind::Number | TokenKind::String | TokenKind::Character
        )
    };
    word(left) && word(right)
}

fn evaluate_expression(
    tokens: &[Token],
    constants: &BTreeMap<String, StructuralValue>,
) -> Option<StructuralValue> {
    let mut parser = ExpressionParser {
        tokens,
        index: 0,
        constants,
    };
    let value = parser.parse_or()?;
    (parser.index == tokens.len()).then_some(value)
}

struct ExpressionParser<'a> {
    tokens: &'a [Token],
    index: usize,
    constants: &'a BTreeMap<String, StructuralValue>,
}

impl ExpressionParser<'_> {
    fn parse_or(&mut self) -> Option<StructuralValue> {
        let mut left = self.parse_and()?;
        while self.take("||") {
            let right = self.parse_and();
            left = match (
                left.as_bool(),
                right.as_ref().and_then(StructuralValue::as_bool),
            ) {
                (Some(true), _) | (_, Some(true)) => StructuralValue::Bool(true),
                (Some(false), Some(false)) => StructuralValue::Bool(false),
                _ => return None,
            };
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<StructuralValue> {
        let mut left = self.parse_equality()?;
        while self.take("&&") {
            let right = self.parse_equality();
            left = match (
                left.as_bool(),
                right.as_ref().and_then(StructuralValue::as_bool),
            ) {
                (Some(false), _) | (_, Some(false)) => StructuralValue::Bool(false),
                (Some(true), Some(true)) => StructuralValue::Bool(true),
                _ => return None,
            };
        }
        Some(left)
    }

    fn parse_equality(&mut self) -> Option<StructuralValue> {
        let left = self.parse_relation()?;
        let operator = if self.take("==") {
            Some(true)
        } else if self.take("!=") {
            Some(false)
        } else {
            None
        };
        let Some(equal) = operator else {
            return Some(left);
        };
        let right = self.parse_relation()?;
        let result = values_equal(&left, &right)?;
        Some(StructuralValue::Bool(if equal { result } else { !result }))
    }

    fn parse_relation(&mut self) -> Option<StructuralValue> {
        let left = self.parse_unary()?;
        let operator = ["<=", ">=", "<", ">"]
            .into_iter()
            .find(|operator| self.take(operator));
        let Some(operator) = operator else {
            return Some(left);
        };
        let right = self.parse_unary()?;
        compare_ranges(&left, &right, operator).map(StructuralValue::Bool)
    }

    fn parse_unary(&mut self) -> Option<StructuralValue> {
        if self.take("!") {
            return self
                .parse_unary()?
                .as_bool()
                .map(|value| StructuralValue::Bool(!value));
        }
        if self.take("-") {
            return self
                .parse_unary()?
                .singleton_integer()
                .map(|value| StructuralValue::Integer(-value));
        }
        if self.take("+") {
            return self.parse_unary();
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<StructuralValue> {
        if self.take("(") {
            let value = self.parse_or()?;
            self.take(")").then_some(value)
        } else {
            let token = self.tokens.get(self.index)?;
            if token.text == "true" || token.text == "false" {
                self.index += 1;
                return Some(StructuralValue::Bool(token.text == "true"));
            }
            if let Some(value) = parse_integer_token(token) {
                self.index += 1;
                return Some(StructuralValue::Integer(value));
            }
            if token.kind != TokenKind::Identifier {
                return None;
            }
            let mut key = token.text.clone();
            self.index += 1;
            while self
                .tokens
                .get(self.index)
                .is_some_and(|token| matches!(token.text.as_str(), "." | "->"))
            {
                self.index += 1;
                let member = self.tokens.get(self.index)?;
                if member.kind != TokenKind::Identifier {
                    return None;
                }
                key.push('.');
                key.push_str(&member.text);
                self.index += 1;
            }
            if self.take("[") {
                let index = self.parse_or()?.singleton_integer()?;
                if !self.take("]") {
                    return None;
                }
                key.push_str(&format!("[{index}]"));
            }
            self.constants
                .get(&key)
                .cloned()
                .or_else(|| Some(StructuralValue::Symbol(key)))
        }
    }

    fn take(&mut self, expected: &str) -> bool {
        if self
            .tokens
            .get(self.index)
            .is_some_and(|token| token.text == expected)
        {
            self.index += 1;
            true
        } else {
            false
        }
    }
}

fn values_equal(left: &StructuralValue, right: &StructuralValue) -> Option<bool> {
    match (left, right) {
        (StructuralValue::Bool(left), StructuralValue::Bool(right)) => Some(left == right),
        (StructuralValue::Integer(left), StructuralValue::Integer(right)) => Some(left == right),
        // Symbols are unresolved runtime expressions, not symbolic constants.
        // Different spellings do not prove different values (and even the same
        // floating-point expression may be NaN), so equality must stay dynamic.
        (StructuralValue::Symbol(_), _) | (_, StructuralValue::Symbol(_)) => None,
        _ => {
            let left = left.singleton_integer()?;
            let right = right.singleton_integer()?;
            Some(left == right)
        }
    }
}

fn compare_ranges(left: &StructuralValue, right: &StructuralValue, operator: &str) -> Option<bool> {
    let (left_first, left_last) = integer_range(left)?;
    let (right_first, right_last) = integer_range(right)?;
    match operator {
        "<" if left_last < right_first => Some(true),
        "<" if left_first >= right_last => Some(false),
        "<=" if left_last <= right_first => Some(true),
        "<=" if left_first > right_last => Some(false),
        ">" if left_first > right_last => Some(true),
        ">" if left_last <= right_first => Some(false),
        ">=" if left_first >= right_last => Some(true),
        ">=" if left_last < right_first => Some(false),
        _ => None,
    }
}

fn integer_range(value: &StructuralValue) -> Option<(i64, i64)> {
    match value {
        StructuralValue::Bool(value) => {
            let value = i64::from(*value);
            Some((value, value))
        }
        StructuralValue::Integer(value) => Some((*value, *value)),
        StructuralValue::IntegerRange { first, last } => Some((*first, *last)),
        StructuralValue::Symbol(_) => None,
    }
}

fn parse_integer_token(token: &Token) -> Option<i64> {
    let text = token
        .text
        .trim_end_matches(|character: char| matches!(character, 'u' | 'U' | 'l' | 'L'));
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse::<i64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constants(entries: &[(&str, StructuralValue)]) -> BTreeMap<String, StructuralValue> {
        entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect()
    }

    #[test]
    fn removes_scene_disabled_branch_without_touching_live_arithmetic() {
        let simplified = simplify_formula_body(
            "if (fractal->common.enabled) { z.x = z.x * 2.0f + 1.0f; } else { z.y += 3.0f; } return z;",
            &constants(&[("fractal.common.enabled", StructuralValue::Bool(false))]),
            None,
            false,
            false,
            false,
        )
        .unwrap();
        assert!(!simplified.source.contains("z . x"));
        assert!(simplified.source.contains("z.y+=3.0f"));
        assert_eq!(simplified.stats.conditions_removed, 1);
    }

    #[test]
    fn preserves_source_bytes_when_no_rewrite_applies() {
        let source =
            "float value = z.x;\n// Retain source layout for stable Metal codegen.\nz.y += value;";
        let simplified =
            simplify_formula_body(source, &BTreeMap::new(), None, false, false, false).unwrap();
        assert_eq!(simplified.source, source);
        assert_eq!(simplified.stats, FormulaSimplificationStats::default());
    }

    #[test]
    fn does_not_fold_equality_between_unresolved_runtime_values() {
        let source = "if (z.x != oldZ.x) aux.color += 1.0f;";
        let simplified =
            simplify_formula_body(source, &BTreeMap::new(), None, false, false, false).unwrap();
        assert_eq!(simplified.source, source);
        assert_eq!(simplified.stats.conditions_removed, 0);
    }

    #[test]
    fn proves_iteration_window_for_a_whole_phase() {
        let constants = constants(&[
            ("fractal.common.start", StructuralValue::Integer(4)),
            ("fractal.common.stop", StructuralValue::Integer(8)),
        ]);
        let active = simplify_formula_body(
            "if (aux->i >= fractal->common.start && aux->i < fractal->common.stop) z.x += 1.0f;",
            &constants,
            Some((4, 7)),
            false,
            false,
            false,
        )
        .unwrap();
        assert!(!active.source.contains("if"));
        assert!(active.source.contains("z.x+=1.0f"));
        let inactive = simplify_formula_body(
            "if (aux->i >= fractal->common.start && aux->i < fractal->common.stop) z.x += 1.0f;",
            &constants,
            Some((8, 12)),
            false,
            false,
            false,
        )
        .unwrap();
        assert!(inactive.source.trim().is_empty());
    }

    #[test]
    fn scalarizes_small_counted_loops_and_local_flag_arrays() {
        let constants = constants(&[
            ("fractal.common.a", StructuralValue::Bool(true)),
            ("fractal.common.b", StructuralValue::Bool(false)),
        ]);
        let simplified = simplify_formula_body(
            "bool enabled[2] = { fractal->common.a, fractal->common.b }; for (int f = 0; f < 2; f++) { if (enabled[f]) z.x += float(f); }",
            &constants,
            None,
            true,
            false,
            false,
        )
        .unwrap();
        assert!(!simplified.source.contains("for"));
        assert!(!simplified.source.contains("if"));
        assert!(simplified.source.contains("float(0)"));
        assert!(!simplified.source.contains("float(1)"));
        assert_eq!(simplified.stats.loops_scalarized, 1);
    }

    #[test]
    fn selects_constant_switch_case_and_preserves_fallthrough_labels() {
        let simplified = simplify_formula_body(
            "switch (fractal->common.mode) { case mode_a: default: z.x += 1.0f; break; case mode_b: z.x += 2.0f; break; }",
            &constants(&[
                ("fractal.common.mode", StructuralValue::Integer(0)),
                ("mode_a", StructuralValue::Integer(0)),
                ("mode_b", StructuralValue::Integer(1)),
            ]),
            None,
            false,
            false,
            false,
        )
        .unwrap();
        assert!(!simplified.source.contains("switch"));
        assert!(simplified.source.contains("z.x+=1.0f"));
        assert!(!simplified.source.contains("z.x+=2.0f"));
        assert_eq!(simplified.stats.switches_removed, 1);
    }

    #[test]
    fn removes_only_unused_declarations_with_pure_initializers() {
        let tokens = lex(
            "float dead = aux->r; float live = sin(aux->r); z.x += live; float side = mutate(z);",
        )
        .unwrap();
        let (simplified, removed) = eliminate_dead_pure_declarations(&tokens).unwrap();
        let source = render_tokens(&simplified);
        assert_eq!(removed, 1);
        assert!(!source.contains("dead"));
        assert!(source.contains("live"));
        assert!(source.contains("mutate"));
    }

    #[test]
    fn reuses_adjacent_identical_pure_initializers_when_values_stay_immutable() {
        let tokens = lex("float a = z.x * z.x; float b = z.x * z.x; z.y += a + b;").unwrap();
        let (simplified, removed) = eliminate_adjacent_common_initializers(&tokens).unwrap();
        let source = render_tokens(&simplified);
        assert_eq!(removed, 1);
        assert!(!source.contains("float b"));
        assert!(source.contains("z.y+=a+a"));

        let tokens = lex("float a = z.x; float b = z.x; a += 1.0f; z.y += b;").unwrap();
        assert_eq!(
            eliminate_adjacent_common_initializers(&tokens).unwrap().1,
            0
        );
    }
}
