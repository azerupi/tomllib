use ast::structs::{Time, FullDate, KeyVal, WSSep, Value, StrType, ErrorCode};
use ::types::{DateTime, TimeOffset, TimeOffsetAmount};
use objects::{array, inline_table};
use parser::{Parser, count_lines};
use nom;
use nom::IResult;

fn is_keychar(&self, chr: char) -> bool {
  let uchr = chr as u32;
  uchr >= 0x41 && uchr <= 0x5A || // A-Z
  uchr >= 0x61 && uchr <= 0x7A || // a-z
  uchr >= 0x30 && uchr <= 0x39 || // 0-9
  uchr == 0x2D || uchr == 0x5f    // "-", "_"
}

impl<'a> Parser<'a> {
  // Integer
  method!(integer<&mut Parser<'a>,&str, &str>, re_find!("^((\\+|-)?(([1-9](\\d|(_\\d))+)|\\d))")) ;

  // Float
  method!(float<&mut Parser<'a>,&str, &str>,
         re_find!("^(\\+|-)?([1-9](\\d|(_\\d))+|\\d)((\\.\\d(\\d|(_\\d))*)((e|E)(\\+|-)?([1-9](\\d|(_\\d))+|\\d))|(\\.\\d(\\d|(_\\d))*)|((e|E)(\\+|-)?([1-9](\\d|(_\\d))+|\\d)))"));

  // String
  // TODO: method!(string<&str, &str>, alt!(basic_string | ml_basic_string | literal_string | ml_literal_string));

  // Basic String
  method!(raw_basic_string<&mut Parser<'a>,&str, &str>,
    re_find!("^\"( |!|[#-\\[]|[\\]-􏿿]|(\\\\\")|(\\\\)|(\\\\/)|(\\b)|(\\f)|(\\n)|(\\r)|(\\t)|(\\\\u[0-9A-Z]{4})|(\\\\U[0-9A-Z]{8}))*?\""));
  // Multiline Basic String
  method!(raw_ml_basic_string<&mut Parser<'a>,&str, &str>,
    chain!(
   string: re_find!("^\"\"\"([ -\\[]|[\\]-􏿿]|(\\\\\")|(\\\\)|(\\\\/)|(\\b)|(\\f)|(\\n)|(\\r)|(\t)|(\\\\u[0-9A-Z]{4})|(\\\\U[0-9A-Z]{8})|\n|(\r\n)|(\\\\(\n|(\r\n))))*?\"\"\""),
      ||{self.line_count += count_lines(string)); string}
    )
  );
  // Literal String
  method!(raw_literal_string<&mut Parser<'a>,&str, &str>,re_find!("^'(	|[ -&]|[\\(-􏿿])*?'"));
  // Multiline Literal String
  method!(raw_ml_literal_string<&mut Parser<'a>,&str, &str>,
    chain!(
   string: re_find!("^'''(	|[ -􏿿]|\n|(\r\n))*?'''"),
      ||{LINE_COUNT.with(|f| *f.borrow_mut() = *f.borrow() + count_lines(string)); string}
    )
  );


  fn ml_basic_string(&self, input: &str) -> nom::IResult<&str, &str> {
    let raw = self.raw_ml_basic_string(input);
    match &raw {
      &IResult::Done(i, o) => IResult::Done(i, &o["\"\"\"".len()..o.len()-"\"\"\"".len()]),
      &IResult::Error(_) => IResult::Error(nom::Err::Code(nom::ErrorKind::Custom(ErrorCode::MLLiteralString as u32))),
      &IResult::Incomplete(i) => IResult::Incomplete(i),
    }
  }

  fn basic_string(&self, input: &str) -> nom::IResult<&str, &str> {
    let raw = self.raw_basic_string(input);
    match &raw {
      &IResult::Done(i, o) => IResult::Done(i, &o["\"".len()..o.len()-"\"".len()]),
      &IResult::Error(_) => IResult::Error(nom::Err::Code(nom::ErrorKind::Custom(ErrorCode::MLLiteralString as u32))),
      &IResult::Incomplete(i) => IResult::Incomplete(i),
    }
  }

  fn ml_literal_string(&self, input: &str) -> nom::IResult<&str, &str> {
    let raw = self.raw_ml_literal_string(input);
    match &raw {
      &IResult::Done(i, o) => IResult::Done(i, &o["'''".len()..o.len()-"'''".len()]),
      &IResult::Error(_) => IResult::Error(nom::Err::Code(nom::ErrorKind::Custom(ErrorCode::MLLiteralString as u32))),
      &IResult::Incomplete(i) => IResult::Incomplete(i),
    }
  }

  fn literal_string(&self, input: &str) -> nom::IResult<&str, &str> {
    let raw = self.raw_literal_string(input);
    match &raw {
      &IResult::Done(i, o) => IResult::Done(i, &o["'".len()..o.len()-"'".len()]),
      &IResult::Error(_) => IResult::Error(nom::Err::Code(nom::ErrorKind::Custom(ErrorCode::MLLiteralString as u32))),
      &IResult::Incomplete(i) => IResult::Incomplete(i),
    }
  }

  method!(string<&mut Parser<'a>,&str, Value>,
    alt!(
      complete_m!(self.ml_literal_string)  => {|ml| Value::String(ml, StrType::MLLiteral)}  |
      complete_m!(self.ml_basic_string)    => {|mb| Value::String(mb, StrType::MLBasic)}  |
      complete_m!(self.basic_string)       => {|b| Value::String(b, StrType::Basic)}    |
      complete_m!(self.literal_string)     => {|l| Value::String(l, StrType::Literal)}
    )
  );

  // Boolean
  method!(boolean<&mut Parser<'a>,&str, &str>, alt!(complete!(tag_s!("false")) | complete!(tag_s!("true"))));


  // Datetime
  // I use re_capture here because I only want the number without the dot. It captures the entire match
  // in the 0th position and the first capture group in the 1st position
  method!(fractional<&mut Parser<'a>,&str, Vec<&str> >, re_capture!("^\\.([0-9]+)"));

  method!(time<&&mut Parser<'a>,str, Time>,
    chain!(
      hour: re_find!("^[0-9]{2}")   ~
            tag_s!(":")             ~
    minute: re_find!("^[0-9]{2}")   ~
            tag_s!(":")             ~
    second: re_find!("^[0-9]{2}")   ~
   fraction: complete_m!(self.fractional)? ,
      ||{
        Time{
          hour: hour, minute: minute, second: second, fraction: match fraction {
            Some(ref x) => x[1],
            None        => "",
          }
        }
      }
    )
  );

  method!(time_offset_amount<&mut Parser<'a>,&str, TimeOffsetAmount>,
    chain!(
  pos_neg: alt!(complete!(tag_s!("+")) | complete!(tag_s!("-")))  ~
     hour: re_find!("^[0-9]{2}")                                                                      ~
           tag_s!(":")                                                                                      ~
  minute: re_find!("^[0-9]{2}")                                                                       ,
      ||{
        TimeOffsetAmount{
          pos_neg: pos_neg, hour: hour, minute: minute
        }
      }
    )
  );

  method!(time_offset<&mut Parser<'a>,&str, TimeOffset>,
    alt!(
      complete!(tag_s!("Z"))                => {|_|       TimeOffset::Z} |
      complete_m!(self.time_offset_amount)  => {|offset|  TimeOffset::Time(offset)}
    )
  );

  method!(full_date<&mut Parser<'a>,&str, FullDate>,
    chain!(
     year: re_find!("^([0-9]{4})") ~
           tag_s!("-") ~
    month: re_find!("^([0-9]{2})") ~
           tag_s!("-") ~
      day: re_find!("^([0-9]{2})"),
      ||{
        FullDate{
          year: year, month: month, day: day
        }
      }
    )
  );

  method!(date_time<&mut Parser<'a>,&str, DateTime>,
    chain_m!(
     date: self.full_date  ~
           tag_s!("T")~
     time: self.time       ~
   offset: self.time_offset,
        ||{
        DateTime{
          year: date.year, month: date.month, day: date.day,
          hour: time.hour, minute: time.minute, second: time.second,
          fraction: time.fraction, offset: offset
        }
      }
    )
  );

  // Key-Value pairs
  method!(unquoted_key<&mut Parser<'a>,&str, &str>, take_while1_s!(is_keychar));
  method!(quoted_key<&mut Parser<'a>,&str, &str>, re_find!("^\"( |!|[#-\\[]|[\\]-􏿿]|(\\\\\")|(\\\\\\\\)|(\\\\/)|(\\\\b)|(\\\\f)|(\\\\n)|(\\\\r)|(\\\\t)|(\\\\u[0-9A-Z]{4})|(\\\\U[0-9A-Z]{8}))+\""));

  method!(pub key<&mut Parser<'a>,&str, &str>, alt!(complete_m!(self.quoted_key) | complete_m!(self.unquoted_key)));

  method!(keyval_sep<&mut Parser<'a>,&str, WSSep>,
    chain_m!(
      ws1: self.ws         ~
           tag_s!("=")~
      ws2: self.ws         ,
      ||{
        WSSep{
          ws1: ws1, ws2: ws2
        }
      }     
    )
  );

  method!(pub val<&mut Parser<'a>,&str, Value>,
    alt!(
      complete_m!(array)        => {|arr|   Value::Array(Box::new(arr))}      |
      complete_m!(inline_table) => {|it|    Value::InlineTable(Box::new(it))} |
      complete_m!(date_time)    => {|dt|    Value::DateTime(dt)}              |
      complete_m!(float)        => {|flt|   Value::Float(flt)}                |
      complete_m!(integer)      => {|int|   Value::Integer(int)}              |
      complete_m!(boolean)      => {|b|     Value::Boolean(b)}                |
      complete_m!(string)       => {|s|     s}
    )
  );

  method!(pub keyval<&mut Parser<'a>,&str, KeyVal>,
    chain_m!(
      key: self.key        ~
       ws: self.keyval_sep ~
      val: self.val        ,
      || {
        KeyVal{
          key: key, keyval_sep: ws, val: val
        }
      }
    )
  );
}

#[cfg(test)]
mod test {
  use nom::IResult::Done;
  use ast::structs::{Time, FullDate, WSSep, Array, ArrayValue, KeyVal,
                     InlineTable, TableKeyVal, Value, StrType};
  use ::types::{DateTime, TimeOffsetAmount, TimeOffset};
  use parser::Parser;

  #[test]
  fn test_integer() {
    assert_eq!(integer("345_12_678"), Done("", "345_12_678"));
  }

  #[test]
  fn test_float() {
    assert_eq!(float("98_7.2_34e-8_8"), Done("", "98_7.2_34e-8_8"));
  }

  #[test]
  fn test_basic_string() {
    assert_eq!(basic_string("\"Tλïƨ ïƨ á βáƨïç ƨƭřïñϱ.\""), Done("", "Tλïƨ ïƨ á βáƨïç ƨƭřïñϱ."));
  }

  #[test]
  fn test_ml_basic_string() {
    assert_eq!(ml_basic_string("\"\"\"£ïñè Óñè
£ïñè Tωô
£ïñè Tλřèè\"\"\""), Done("", r#"£ïñè Óñè
£ïñè Tωô
£ïñè Tλřèè"# ));
  }

  #[test]
  fn test_literal_string() {
    assert_eq!(literal_string("'Abc џ'"), Done("", "Abc џ")); 
  }

  #[test]
  fn test_ml_literal_string() {
    assert_eq!(ml_literal_string(r#"'''
                                    Abc џ
                                    '''"#),
      Done("", r#"
                                    Abc џ
                                    "#));
  }

  #[test]
  fn test_foo() {
    assert!(true);
  }

  #[test]
  fn test_string() {
    assert_eq!(string("\"βáƨïç_ƨƭřïñϱ\""), Done("", Value::String("βáƨïç_ƨƭřïñϱ", StrType::Basic)));
assert_eq!(string(r#""""₥ℓ_βáƨïç_ƨƭřïñϱ
ñú₥βèř_ƭωô
NÛMßÉR-THRÉÉ
""""#), Done("", Value::String(r#"₥ℓ_βáƨïç_ƨƭřïñϱ
ñú₥βèř_ƭωô
NÛMßÉR-THRÉÉ
"#, StrType::MLBasic)));
    assert_eq!(string("'£ÌTÉRÂ£§TRïNG'"), Done("", Value::String("£ÌTÉRÂ£§TRïNG", StrType::Literal)));
    assert_eq!(string(r#"'''§ƥřïƭè
Çôƙè
Þèƥƨï
'''"#),
      Done("", Value::String(r#"§ƥřïƭè
Çôƙè
Þèƥƨï
"#, StrType::MLLiteral)));

  }

  #[test]
  fn test_boolean() {
    assert_eq!(boolean("true"), Done("", "true"));
    assert_eq!(boolean("false"), Done("", "false"));
  }

  #[test]
  fn test_fractional() {
    assert_eq!(fractional(".03856"), Done("", vec![".03856", "03856"]));
  }

  #[test]
  fn test_time() {
    assert_eq!(time("11:22:33.456"),
      Done("", Time{
        hour: "11",
        minute: "22",
        second: "33",
        fraction: "456"
      })
    );
    assert_eq!(time("04:05:06"),
      Done("", Time{
        hour: "04",
        minute: "05",
        second: "06",
        fraction: ""
      })
    );
  }

  #[test]
  fn test_time_offset_amount() {
    assert_eq!(time_offset_amount("+12:34"),
      Done("", TimeOffsetAmount{
        pos_neg: "+",
        hour: "12",
        minute: "34"
      })
    );
  }

  #[test]
  fn test_time_offset() {
    assert_eq!(time_offset("+12:34"),
      Done("", TimeOffset::Time(TimeOffsetAmount{
        pos_neg: "+",
        hour: "12",
        minute: "34"
      }))
    );
    assert_eq!(time_offset("Z"), Done("", TimeOffset::Z));
  }

  #[test]
  fn test_full_date() {
    assert_eq!(full_date("1942-12-07"),
      Done("", FullDate{
        year: "1942", month: "12", day: "07"
      })
    );
  }

  #[test]
  fn test_date_time() {
    assert_eq!(date_time("1999-03-21T20:15:44.5-07:00"),
      Done("", DateTime{
        year: "1999", month: "03", day: "21",
        hour: "20", minute: "15", second: "44", fraction: "5",
        offset: TimeOffset::Time(TimeOffsetAmount{
          pos_neg: "-",
          hour: "07",
          minute: "00"
        })
      })
    );
  }

  #[test]
  fn test_unquoted_key() {
    assert_eq!(unquoted_key("Un-Quoted_Key"), Done("", "Un-Quoted_Key"));
  }

  #[test]
  fn test_quoted_key() {
    assert_eq!(quoted_key("\"QúôƭèδKè¥\""), Done("", "\"QúôƭèδKè¥\""));
  }

  #[test]
  fn test_key() {
    assert_eq!(key("\"Gřáƥèƒřúïƭ\""), Done("", "\"Gřáƥèƒřúïƭ\""));
    assert_eq!(key("_is-key"), Done("", "_is-key"));
  }

  #[test]
  fn test_keyval_sep() {
    assert_eq!(keyval_sep("\t \t= \t"), Done("", WSSep{ws1: "\t \t", ws2: " \t"}));
  }

  #[test]
  fn test_val() {
    assert_eq!(val("[4,9]"), Done("",
      Value::Array(Box::new(Array{
        values: vec![
          ArrayValue{
            val: Value::Integer("4"), array_sep: Some(WSSep{
              ws1: "", ws2: ""
            }),
            comment_nl: None
          },
          ArrayValue{
            val: Value::Integer("9"), array_sep: None,
            comment_nl: None
          },
        ],
        ws: WSSep{ws1: "", ws2: ""}
      }
    ))));

    assert_eq!(val("{\"§ô₥è Þïϱ\"='Táƨƭ¥ Þôřƙ'}"), Done("",
      Value::InlineTable(Box::new(InlineTable{
        keyvals: Some(vec![
          TableKeyVal{
            keyval: KeyVal{
              key: "\"§ô₥è Þïϱ\"", keyval_sep: WSSep{
                ws1: "", ws2: ""
              },
              val: Value::String("Táƨƭ¥ Þôřƙ", StrType::Literal)
            },
            kv_sep: WSSep{ws1: "", ws2: ""}
          }
        ]),
        ws: WSSep{
          ws1: "", ws2: ""
        }
    }))));

    assert_eq!(val("2112-09-30T12:33:01.345-11:30"), Done("", Value::DateTime(DateTime{
                              year: "2112", month: "09", day: "30",
                              hour: "12", minute: "33", second: "01", fraction: "345",
                              offset: TimeOffset::Time(TimeOffsetAmount{
                                pos_neg: "-", hour: "11", minute: "30"
                              })
                            })));
    assert_eq!(val("3487.3289E+22"), Done("", Value::Float("3487.3289E+22")));
    assert_eq!(val("8932838"), Done("", Value::Integer("8932838")));
    assert_eq!(val("false"), Done("", Value::Boolean("false")));
    assert_eq!(val("true"), Done("", Value::Boolean("true")));
    assert_eq!(val("'§ô₥è §ƭřïñϱ'"), Done("", Value::String("§ô₥è §ƭřïñϱ", StrType::Literal)));
  }

  #[test]
  fn test_keyval() {
    assert_eq!(keyval("Boolean = 84.67"), Done("", KeyVal{
      key: "Boolean", keyval_sep: WSSep{
        ws1: " ", ws2: " "
      },
      val: Value::Float("84.67")
    }));
  }
}