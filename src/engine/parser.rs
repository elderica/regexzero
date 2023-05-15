//! 正規表現の式をパースし、抽象構文木に変換。
use std::{
    error::Error,
    fmt::{self, Display},
    mem::take,
};

/// 抽象構文木を表現するための型。
#[derive(Debug)]
pub enum AST {
    Char(char),
    Plus(Box<AST>),
    Star(Box<AST>),
    Question(Box<AST>),
    Or(Box<AST>, Box<AST>),
    Seq(Vec<AST>),
}

/// パースエラーを表すための型。
#[derive(Debug)]
pub enum ParseError {
    InvalidEscape(usize, char), // 誤ったエスケープシーケンス
    InvalidRightParen(usize),   // 開き括弧なし
    NoPrev(usize),              // +、|、*、?の前に式がない
    NoRightParen,               // 閉じ括弧なし
    Empty,                      // 空のパターン
}

/// パースエラーを表示するために、 Display トレイトを実装。
impl Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidEscape(pos, c) => {
                write!(f, "ParseError: invalid escape: pos = {pos}, char = '{c}'")
            }
            ParseError::InvalidRightParen(pos) => {
                write!(f, "ParseError: invalid right parenthesis: pos = {pos}")
            }
            ParseError::NoPrev(pos) => {
                write!(f, "ParseError: no previous expression: pos = {pos}")
            }
            ParseError::NoRightParen => {
                write!(f, "ParseError: no right parenthesis")
            }
            ParseError::Empty => {
                write!(f, "ParseError: empty expression")
            }
        }
    }
}

impl Error for ParseError {}

/// 特殊文字のエスケープ。
fn parse_escape(pos: usize, c: char) -> Result<AST, ParseError> {
    match c {
        '\\' | '(' | ')' | '|' | '+' | '*' | '?' => Ok(AST::Char(c)),
        _ => {
            let err = ParseError::InvalidEscape(pos, c);
            Err(err)
        }
    }
}

/// parse_plus_star_question 関数で利用するための列挙型。
enum PSQ {
    Plus,
    Star,
    Question,
}

/// +, *, ?を AST に変換。
///
/// 後置記法で、+、*、?の前にパターンがないとエラー。
/// 例 : *ab、abc|+などはエラー。
fn parse_plus_star_question(
    seq: &mut Vec<AST>,
    ast_type: PSQ,
    pos: usize,
) -> Result<(), ParseError> {
    if let Some(prev) = seq.pop() {
        let ast = match ast_type {
            PSQ::Plus => AST::Plus(Box::new(prev)),
            PSQ::Star => AST::Star(Box::new(prev)),
            PSQ::Question => AST::Question(Box::new(prev)),
        };
        seq.push(ast);
        Ok(())
    } else {
        Err(ParseError::NoPrev(pos))
    }
}

/// Or で結合された複数の式を AST に変換。
///
/// たとえば、abc|def|ghiは、 AST::Or("abc", AST::Or("def, "ghi")) という AST となる。
fn fold_or(mut seq_or: Vec<AST>) -> Option<AST> {
    if seq_or.len() > 1 {
        // seq_or の要素が複数ある場合は、 Or で式を結合
        let mut ast = seq_or.pop().unwrap();
        seq_or.reverse();
        for s in seq_or {
            ast = AST::Or(Box::new(s), Box::new(ast));
        }
        Some(ast)
    } else {
        // seq_or の要素が1つのみの場合は、 Or ではなく、最初の値を返す
        seq_or.pop()
    }
}

/// 正規表現を抽象構文木に変換。
pub fn parse(expr: &str) -> Result<AST, ParseError> {
    // 内部状態を表現するための型
    // Char 状態 : 文字列処理中
    // Escape 状態 : エスケープシーケンス処理中
    enum ParseState {
        Char,
        Escape,
    }

    let mut seq = Vec::new(); // 現在の Seq のコンテキスト
    let mut seq_or = Vec::new(); // 現在の Or のコンテキスト
    let mut stack = Vec::new(); // コンテキストのスタック
    let mut state = ParseState::Char; // 現在の状態

    for (i, c) in expr.chars().enumerate() {
        match &state {
            ParseState::Char => {
                match c {
                    '+' => parse_plus_star_question(&mut seq, PSQ::Plus, i)?,
                    '*' => parse_plus_star_question(&mut seq, PSQ::Star, i)?,
                    '?' => parse_plus_star_question(&mut seq, PSQ::Question, i)?,
                    '(' => {
                        // 現在のコンテキストをスタックに保存し、
                        // 現在のコンテキストを空の状態にする
                        let prev = take(&mut seq);
                        let prev_or = take(&mut seq_or);
                        stack.push((prev, prev_or));
                    }
                    ')' => {
                        // 現在のコンテキストをスタックからポップ
                        if let Some((mut prev, prev_or)) = stack.pop() {
                            // "()"のように、式が空の場合は push しない
                            if !seq.is_empty() {
                                seq_or.push(AST::Seq(seq))
                            }

                            // Or を生成
                            if let Some(ast) = fold_or(seq_or) {
                                prev.push(ast);
                            }

                            // 以前のコンテキストを、現在のコンテキストにする
                            seq = prev;
                            seq_or = prev_or;
                        } else {
                            // "abc)" のように、開き括弧がないのに閉じ括弧がある場合はエラー
                            return Err(ParseError::InvalidRightParen(i));
                        }
                    }
                    '|' => {
                        if seq.is_empty() {
                            // "||", "(|abc)"などと、式が空の場合はエラー
                            return Err(ParseError::NoPrev(i));
                        } else {
                            let prev = take(&mut seq);
                            seq_or.push(AST::Seq(prev));
                        }
                    }
                    '\\' => {
                        state = ParseState::Escape;
                    }
                    _ => seq.push(AST::Char(c)),
                };
            }
            ParseState::Escape => {
                // エスケープシーケンス処理
                let ast = parse_escape(i, c)?;
                seq.push(ast);
                state = ParseState::Char;
            }
        };
    }

    // 閉じ括弧が足りない場合はエラー
    if !stack.is_empty() {
        return Err(ParseError::NoRightParen);
    }

    // "()"のように式が空の場合はpushしない
    if !seq.is_empty() {
        seq_or.push(AST::Seq(seq));
    }

    // Or を生成し、成功した場合はそれを返す
    if let Some(ast) = fold_or(seq_or) {
        Ok(ast)
    } else {
        Err(ParseError::Empty)
    }
}
