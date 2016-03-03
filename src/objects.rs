use ast::structs::{TableType, WSKeySep, Table, CommentNewLines,
                   CommentOrNewLines, ArrayValue, Array, Value,
                   InlineTable, WSSep, TableKeyVal, ArrayType,
                   HashValue, format_tt_keys};
use parser::{Parser, Key};
use types::{ParseError, Str, Children};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::cell::Cell;
use nom::IResult;

#[inline(always)]
fn map_val_to_array_type(val: &Value) -> ArrayType {
  match val {
    &Value::Integer(_)        => ArrayType::Integer,
    &Value::Float(_)          => ArrayType::Float,
    &Value::Boolean(_)        => ArrayType::Boolean,
    &Value::DateTime(_)       => ArrayType::DateTime,
    &Value::Array(_)          => ArrayType::Array,
    &Value::String(_,_)       => ArrayType::String,
    &Value::InlineTable(_)    => ArrayType::InlineTable,
  }
}

impl<'a> Parser<'a> {

  fn is_top_std_table(tables: &RefCell<Vec<Rc<TableType<'a>>>>) -> bool {
    if tables.borrow().len() ==  0 {
      return false;
    } else {
      let len = tables.borrow().len();
      if let TableType::Standard(_) = *tables.borrow()[len - 1] {
        return true;
      } else {
        return false;
      }
    }
  }

  fn equal_key_length(table: Rc<TableType<'a>>, tables: &RefCell<Vec<Rc<TableType<'a>>>>) -> bool {
    if tables.borrow().len() ==  0 {
      return false;
    } else {
      match *table {
        TableType::Array(ref t1) | TableType::Standard(ref t1) => {
          let len = tables.borrow().len();
          match *tables.borrow()[len - 1] {
            TableType::Array(ref t2) | TableType::Standard(ref t2) => {
              return t1.keys.len() == t2.keys.len();
            }
          }
        }
      }
    }
  }

  fn add_implicit_tables(map: &RefCell<&mut HashMap<String, HashValue<'a>>>,
    tables: &RefCell<Vec<Rc<TableType<'a>>>>,
    tables_index: &RefCell<Vec<usize>>, table: Rc<TableType<'a>>) {
    let mut last_key = Parser::get_array_table_key(map, tables, tables_index);
    let mut len = tables.borrow().len();
    let mut pop = false;
    // TODO: Need to add an array_table_root key that points to all of it's Children
    //       And a std_table_root that points to all of it's children, neither root should
    //       be part of the key
    if len == 0 {
      tables.borrow_mut().push(Rc::new(TableType::Standard(
        Table::new_str(WSSep::new_str("", ""), "$TableRoot$", vec![])
      )));
      pop = true;
      len = 1;
      last_key.push_str("$TableRoot$");
    }
    match *tables.borrow()[len - 1] {
      TableType::Array(ref last_at) | TableType::Standard(ref last_at) => {
    // if let TableType::Array(ref last_at) = *tables.borrow()[len - 1] || let TableType::Standard(ref last_at) = *tables.borrow()[len - 1]
    // {
        match *table {
          TableType::Array(ref tb) | TableType::Standard(ref tb) => {
            let mut first = true;
            println!("last_at.keys.len() - 1: {}, tb.keys.len() - 1: {}", last_at.keys.len() - 1, tb.keys.len() - 1);
            for i in 0..last_at.keys.len() {
              println!("key {}: {}", i, last_at.keys[i].key);
            }
            let mut start = last_at.keys.len();
            if last_key == "$TableRoot$" {
              start -= 1;
            }
            for i in start..tb.keys.len() {
              let mut borrow = map.borrow_mut();
              let mut insert = false;
              println!("index: {}, last_key: {}", i, last_key);
              if let Entry::Occupied(mut o) = borrow.entry(last_key.clone()) {
                if first {
                  insert = match &o.get_mut().subkeys {
                    &Children::Keys(ref hs_rf) => hs_rf.borrow_mut().insert(string!(tb.keys[i].key)),
                    &Children::Count(ref cell) => { cell.set(cell.get() + 1); true },
                  };
                  first = false;
                } else {
                  insert = match &o.get_mut().subkeys {
                    &Children::Keys(ref hs_rf) => hs_rf.borrow_mut().insert(string!(tb.keys[i].key)),
                    _ => panic!("Implicit tables can only be Standard Tables: \"{}\"", format!("{}.{}", last_key, str!(tb.keys[i].key))),
                  };
                }
              }
              if last_key != "$TableRoot$" {
                last_key.push_str(".");
              } else {
                last_key.truncate(0);
              }
              last_key.push_str(str!(tb.keys[i].key));
              if insert {
                println!("insert last_key {}", last_key);
                if i == tb.keys.len() - 1 {
                  if let TableType::Array(_) = *table {
                    borrow.insert(last_key.clone(), HashValue::one_count());
                  } else {
                    borrow.insert(last_key.clone(), HashValue::none_keys());
                  }
                } else {
                  borrow.insert(last_key.clone(), HashValue::none_keys());
                }
              }
            }
          },
        }
      }
    }
    if pop {
      tables.borrow_mut().pop();
    }
    println!("Returning from add_implicit_tables");
  }

  fn increment_array_table_index(map: &RefCell<&mut HashMap<String, HashValue<'a>>>,
    tables: &RefCell<Vec<Rc<TableType<'a>>>>, tables_index: &RefCell<Vec<usize>>,) {
    let parent_key = Parser::get_key_parent(tables, tables_index);
    println!("increment_array_table_index: {}", parent_key);
    let mut borrow = map.borrow_mut();
    let entry = borrow.entry(parent_key);
    if let Entry::Occupied(mut o) = entry {
      if let &Children::Count(ref c) = &o.get_mut().subkeys {
         c.set(c.get() + 1);
      }
    }
    let len = tables_index.borrow().len();
    let last_index = tables_index.borrow()[len - 1];
    tables_index.borrow_mut()[len - 1] = last_index + 1;
  }

  fn add_to_table_set(map: &RefCell<&mut HashMap<String, HashValue<'a>>>,
    tables: &RefCell<Vec<Rc<TableType<'a>>>>, tables_index: &RefCell<Vec<usize>>, key: &str) -> bool{
    let parent_key = Parser::get_key_parent(tables, tables_index);
    println!("add_to_table_set: {}", parent_key);
    let mut borrow = map.borrow_mut();
    let entry = borrow.entry(parent_key);
    if let Entry::Occupied(mut o) = entry {
      if let &Children::Keys(ref keys) = &o.get_mut().subkeys {
        let contains = keys.borrow().contains(key);
        if contains {
          println!("key already exists");
          return false;
        } else {
          println!("add_to_table_set--> {}", key);
          keys.borrow_mut().insert(key.to_string());
        }
      }
    }
    return true;
  }

  // Table
  method!(pub table<Parser<'a>, &'a str, Rc<TableType> >, mut self,
    alt!(
      complete!(call_m!(self.array_table)) |
      complete!(call_m!(self.std_table))
    )
  );

  method!(table_subkeys<Parser<'a>, &'a str, Vec<WSKeySep> >, mut self, many0!(call_m!(self.table_subkey)));

  method!(table_subkey<Parser<'a>, &'a str, WSKeySep>, mut self,
    chain!(
      ws1: call_m!(self.ws)         ~
           tag_s!(".")~
      ws2: call_m!(self.ws)         ~
      key: call_m!(self.key)        ,
      ||{
        WSKeySep::new_str(WSSep::new_str(ws1, ws2), key)
      } 
    )
  );
  // Standard Table
  method!(std_table<Parser<'a>, &'a str, Rc<TableType> >, mut self,
    chain!(
           tag_s!("[")    ~
      ws1: call_m!(self.ws)             ~
      key: call_m!(self.key)            ~
  subkeys: call_m!(self.table_subkeys)  ~
      ws2: call_m!(self.ws)             ~
           tag_s!("]")    ,
      ||{
        let keys_len = subkeys.len() + 1;
        let res = Rc::new(TableType::Standard(Table::new_str(
          WSSep::new_str(ws1, ws2), key, subkeys
        )));
        let mut error = false;
        let keychain_len = self.keychain.borrow().len();
        self.keychain.borrow_mut().truncate(keychain_len - keys_len);
        if Parser::is_top_std_table(&self.last_array_tables) || Parser::equal_key_length(res.clone(), &self.last_array_tables) {
          self.last_array_tables.borrow_mut().pop();
          self.last_array_tables_index.borrow_mut().pop();
        }
        let map = RefCell::new(&mut self.map);
        let mut table_key = "".to_string();
        println!("Before get len");
        let mut len = self.last_array_tables.borrow().len();
        if len > 0 {
          let current_key = format_tt_keys(&*res);
          let last_key = format_tt_keys(&self.last_array_tables.borrow()[len - 1]);
          if current_key == last_key {
            error = true;
          } else {
            println!("Check if subtable");
            let subtable = res.is_subtable_of(&self.last_array_tables.borrow()[len - 1]);
            if !subtable {
              loop {
                println!("Not subtable pop {}", self.last_array_tables.borrow()[self.last_array_tables.borrow().len() - 1]);
                self.last_array_tables.borrow_mut().pop();
                self.last_array_tables_index.borrow_mut().pop();
                len -= 1;
                println!("check array_tables len and subtable");
                if len == 0 || res.is_subtable_of(&self.last_array_tables.borrow()[len - 1]) {
                  break;
                }
              }
            }
            if len > 0 && current_key == format_tt_keys(&self.last_array_tables.borrow()[len - 1]) {
              error = true;
            } else {
              self.last_array_tables.borrow_mut().push(res.clone());
              self.last_array_tables_index.borrow_mut().push(0);
              table_key = Parser::get_array_table_key(&map, &self.last_array_tables, &self.last_array_tables_index);
              self.last_array_tables.borrow_mut().pop();
              self.last_array_tables_index.borrow_mut().pop();
              println!("Standard Table Key: {}", table_key);
              if map.borrow().contains_key(&table_key) {
                let map_borrow = map.borrow();
                let hash_val_opt = map_borrow.get(&table_key);
                if let Some(ref hash_val) = hash_val_opt {
                  if let Children::Count(_) = hash_val.subkeys {
                    error = true;
                  } else if let Children::Keys(ref keys) = hash_val.subkeys {
                    if keys.borrow().len() > 0 {
                      error = true;
                    }
                  }
                }
              }
            }
          }
        }
        println!("Before error check");
        if error {
          self.errors.borrow_mut().push(ParseError::InvalidTable (
            format_tt_keys(&res), RefCell::new(HashMap::new())
          ));
          self.array_error.set(true);
        } else {
          Parser::add_implicit_tables(&map, &self.last_array_tables,
            &self.last_array_tables_index, res.clone());
          if let TableType::Standard(ref tbl) = *res {
            Parser::add_to_table_set(&map, &self.last_array_tables,
              &self.last_array_tables_index, str!(tbl.keys[keys_len - 1].key));
            self.array_error.set(false);
            map.borrow_mut().insert(table_key, HashValue::none_keys());
            self.last_array_tables.borrow_mut().push(res.clone());
            self.last_array_tables_index.borrow_mut().push(0);
            self.last_table = Some(res.clone());
          }
        }
        res
      }
    )
  );

  //Array Table
  method!(array_table<Parser<'a>, &'a str, Rc<TableType> >, mut self,
    chain!(
           tag_s!("[[")   ~
      ws1: call_m!(self.ws)             ~
      key: call_m!(self.key)            ~
  subkeys: call_m!(self.table_subkeys)  ~
      ws2: call_m!(self.ws)             ~
           tag_s!("]]")   ,
      ||{
        let keys_len = subkeys.len() + 1;
        let res = Rc::new(TableType::Array(Table::new_str(
          WSSep::new_str(ws1, ws2), key, subkeys
        )));
        let keychain_len = self.keychain.borrow().len();
        self.keychain.borrow_mut().truncate(keychain_len - keys_len);
        if Parser::is_top_std_table(&self.last_array_tables) {
          self.last_array_tables.borrow_mut().pop();
          self.last_array_tables_index.borrow_mut().pop();
        }
        {
          let map = RefCell::new(&mut self.map);
          self.array_error.set(false);
          let len = self.last_array_tables.borrow().len();
          let current_key = format_tt_keys(&*res);
          if len > 0 {
            let mut len = self.last_array_tables.borrow().len();
            let last_key = format_tt_keys(&self.last_array_tables.borrow()[len - 1]);
            println!("current_key: {}, last_key: {}", current_key, last_key);
            if current_key == last_key {
              println!("Increment array table index");
              Parser::increment_array_table_index(&map, &self.last_array_tables,
                &self.last_array_tables_index);
            } else {
              let subtable = res.is_subtable_of(&self.last_array_tables.borrow()[len - 1]);
              if subtable {
                println!("Is subtable");
                Parser::add_implicit_tables(&map, &self.last_array_tables,
                  &self.last_array_tables_index, res.clone());
                self.last_array_tables.borrow_mut().push(res.clone());
                self.last_array_tables_index.borrow_mut().push(0);
              } else {
                println!("NOT subtable");
                while self.last_array_tables.borrow().len() > 0 &&
                  current_key != format_tt_keys(&self.last_array_tables.borrow()[self.last_array_tables.borrow().len() - 1]) {
                  println!("pop table");
                  self.last_array_tables.borrow_mut().pop();
                  self.last_array_tables_index.borrow_mut().pop();
                }
                len = self.last_array_tables.borrow().len();
                if len > 0 {
                  println!("Increment array table index the second");
                  Parser::increment_array_table_index(&map, &self.last_array_tables,
                    &self.last_array_tables_index);
                } else {
                  println!("Add implicit tables");
                  Parser::add_implicit_tables(&map, &self.last_array_tables,
                    &self.last_array_tables_index,  res.clone());
                  self.last_array_tables.borrow_mut().push(res.clone());
                  self.last_array_tables_index.borrow_mut().push(0);
                }
              }
            }
          } else {
            println!("Len == 0 add implicit tables");
            Parser::add_implicit_tables(&map, &self.last_array_tables,
              &self.last_array_tables_index, res.clone());
            self.last_array_tables.borrow_mut().push(res.clone());
            self.last_array_tables_index.borrow_mut().push(0);
          }
          println!("Before call to get_array_table_key");
          let full_key = Parser::get_array_table_key(&map, &self.last_array_tables,
            &self.last_array_tables_index);
          println!("After call to get_array_table_key");
          let contains_key = map.borrow().contains_key(&full_key);
          if !contains_key {
            map.borrow_mut().insert(full_key, HashValue::none_keys());
          } else {
            Parser::increment_array_table_index(&map, &self.last_array_tables,
              &self.last_array_tables_index);
          }
          self.last_table = Some(res.clone());
        }
        self.print_keys_and_values();
        res
      }
    )
  );

  // Array
  method!(array_sep<Parser<'a>, &'a str, WSSep>, mut self,
    chain!(
      ws1: call_m!(self.ws)         ~
           tag_s!(",")~
      ws2: call_m!(self.ws)         ,
      ||{
        WSSep::new_str(ws1, ws2)
      }
    )
  );

  method!(ws_newline<Parser<'a>, &'a str, &'a str>, self, re_find!("^( |\t|\n|(\r\n))*"));

  method!(comment_nl<Parser<'a>, &'a str, CommentNewLines>, mut self,
    chain!(
   prewsnl: call_m!(self.ws_newline)  ~
   comment: call_m!(self.comment)     ~
  newlines: call_m!(self.ws_newline) ,
      ||{
        CommentNewLines::new_str(prewsnl, comment, newlines)
      }
    )
  );

  method!(comment_or_nl<Parser<'a>, &'a str, CommentOrNewLines>, mut self,
    alt!(
      complete!(call_m!(self.comment_nl))   => {|com| CommentOrNewLines::Comment(com)} |
      complete!(call_m!(self.ws_newline))  => {|nl|  CommentOrNewLines::NewLines(Str::Str(nl))}
    )
  );

  method!(comment_or_nls<Parser<'a>, &'a str, Vec<CommentOrNewLines> >, mut self,
    many1!(call_m!(self.comment_or_nl)));
  
  method!(array_value<Parser<'a>, &'a str, ArrayValue>, mut self,
        chain!(
          val: call_m!(self.val)                        ~
    array_sep: complete!(call_m!(self.array_sep))?      ~
  comment_nls: complete!(call_m!(self.comment_or_nls))  ,
          ||{
            let t = map_val_to_array_type(&*val.borrow());
            let len = self.last_array_type.borrow().len();
            if len > 0 && self.last_array_type.borrow()[len - 1] != ArrayType::None &&
               self.last_array_type.borrow()[len - 1] != t {
              self.mixed_array.set(true);
            }
            self.last_array_type.borrow_mut().pop();
            self.last_array_type.borrow_mut().push(t);
            let keychain_len = self.keychain.borrow().len();
            self.insert_keyval_into_map(val.clone());
            self.keychain.borrow_mut()[keychain_len - 1].inc();
            ArrayValue::new(val, array_sep, comment_nls)
          }
        )
  );

  method!(array_values<Parser<'a>, &'a str, Vec<ArrayValue> >, mut self,
    chain!(
     vals: many0!(call_m!(self.array_value)) ,
     ||{
        println!("Finished array values");
        let mut tmp = vec![];
        tmp.extend(vals);
        tmp
      }
    )
  );

  pub fn array(mut self: Parser<'a>, input: &'a str) -> (Parser<'a>, IResult<&'a str, Rc<RefCell<Array>>>) {
    // Initialize last array type to None, we need a stack because arrays can be nested
    println!("*** array called on input:\t\t\t{}", input);
    self.last_array_type.borrow_mut().push(ArrayType::None);
    self.keychain.borrow_mut().push(Key::Index(Cell::new(0)));
    let (tmp, res) = self.array_internal(input);
    self = tmp; // Restore self
    self.keychain.borrow_mut().pop();
    self.last_array_type.borrow_mut().pop();
    (self, res)
  }

  method!(pub array_internal<Parser<'a>, &'a str, Rc<RefCell<Array>> >, mut self,
    chain!(
              tag_s!("[")                   ~
         cn1: call_m!(self.comment_or_nls)  ~
  array_vals: call_m!(self.array_values)    ~
         cn2: call_m!(self.comment_or_nls)  ~
              tag_s!("]")                   ,
      ||{
        println!("Close array");
       let array_result = Rc::new(RefCell::new(Array::new(array_vals, cn1, cn2)));
        if self.mixed_array.get() {
          self.mixed_array.set(false);
          let mut vals: Vec<Rc<RefCell<Value<'a>>>> = vec![]; 
          for x in 0..array_result.borrow().values.len() {
            vals.push(array_result.borrow().values[x].val.clone());
          }
          self.errors.borrow_mut().push(ParseError::MixedArray(vals));
        }
        array_result
      }
    )
  );

  method!(table_keyval<Parser<'a>, &'a str, TableKeyVal>, mut self,
        chain!(
          ws1: call_m!(self.ws)     ~
       keyval: call_m!(self.keyval) ~
          ws2: call_m!(self.ws)     ,
          ||{
            TableKeyVal::new(keyval, WSSep::new_str(ws1, ws2))
          }
        )
  );

  method!(inline_table_keyvals_non_empty<Parser<'a>, &'a str, Vec<TableKeyVal> >, mut self, separated_list!(tag_s!(","), call_m!(self.table_keyval)));

  method!(pub inline_table<Parser<'a>, &'a str, Rc<RefCell<InlineTable>> >, mut self,
    chain!(
           tag_s!("{")                                ~
      ws1: call_m!(self.ws)                                         ~
  keyvals: complete!(call_m!(self.inline_table_keyvals_non_empty))? ~
      ws2: call_m!(self.ws)                                         ~
           tag_s!("}")                                ,
          ||{
            if let Some(_) = keyvals {
              Rc::new(RefCell::new(InlineTable::new(keyvals.unwrap(), WSSep::new_str(ws1, ws2))))
            } else {
              Rc::new(RefCell::new(InlineTable::new(vec![], WSSep::new_str(ws1, ws2))))
            }
          }
    )
  );
}

#[cfg(test)]
mod test {
  use nom::IResult::Done;
  use ast::structs::{Array, ArrayValue, WSSep, TableKeyVal, InlineTable, WSKeySep,
                     KeyVal, CommentNewLines, Comment, CommentOrNewLines, Table,
                     TableType, Value};
  use ::types::{DateTime, Date, Time, TimeOffset, TimeOffsetAmount, StrType, Str};
  use parser::{Parser, Key};
  use std::rc::Rc;
  use std::cell::{RefCell, Cell};

  #[test]
  fn test_table() {
    let mut p = Parser::new();
    assert_eq!(p.table("[ _underscore_ . \"-δáƨλèƨ-\" ]").1, Done("",
      Rc::new(TableType::Standard(Table::new_str(
        WSSep::new_str(" ", " "), "_underscore_", vec![
          WSKeySep::new_str(WSSep::new_str(" ", " "), "\"-δáƨλèƨ-\"")
        ]
      ))
    )));
    p = Parser::new();
    assert_eq!(p.table("[[\t NumberOne\t.\tnUMBERtWO \t]]").1, Done("",
      Rc::new(TableType::Array(Table::new_str(
        WSSep::new_str("\t ", " \t"), "NumberOne", vec![
          WSKeySep::new_str(WSSep::new_str("\t", "\t"), "nUMBERtWO")
        ]
      ))
    )));
  }

  #[test]
  fn test_table_subkey() {
    let p = Parser::new();
    assert_eq!(p.table_subkey("\t . \t\"áƭúƨôèλôñèƭúññèôúñôèƭú\"").1, Done("",
      WSKeySep::new_str(WSSep::new_str("\t ", " \t"), "\"áƭúƨôèλôñèƭúññèôúñôèƭú\""),
    ));
  }

  #[test]
  fn test_table_subkeys() {
    let p = Parser::new();
    assert_eq!(p.table_subkeys(" .\tAPPLE.MAC . \"ßÓÓK\"").1, Done("",
      vec![
        WSKeySep::new_str(WSSep::new_str(" ", "\t"), "APPLE"),
        WSKeySep::new_str(WSSep::new_str("", ""), "MAC"),
        WSKeySep::new_str(WSSep::new_str(" ", " "), "\"ßÓÓK\"")
      ]
    ));
  }

  #[test]
  fn test_std_table() {
    let p = Parser::new();
    assert_eq!(p.std_table("[Dr-Pepper  . \"ƙè¥_TWÓ\"]").1, Done("",
      Rc::new(TableType::Standard(Table::new_str(
        WSSep::new_str("", ""), "Dr-Pepper", vec![
          WSKeySep::new_str(WSSep::new_str("  ", " "), "\"ƙè¥_TWÓ\"")
        ]
      )))
    ));
  }

  #[test]
  fn test_array_table() {
    let p = Parser::new();
    assert_eq!(p.array_table("[[\"ƙè¥ôñè\"\t. key_TWO]]").1, Done("",
      Rc::new(TableType::Array(Table::new_str(
        WSSep::new_str("", ""), "\"ƙè¥ôñè\"", vec![
          WSKeySep::new_str(WSSep::new_str("\t", " "), "key_TWO")
        ]
      ))
    )));
  }

  #[test]
  fn test_array_sep() {
    let p = Parser::new();
    assert_eq!(p.array_sep("  ,  ").1, Done("", WSSep::new_str("  ", "  ")));
  }

  #[test]
  fn test_ws_newline() {
    let p = Parser::new();
    assert_eq!(p.ws_newline("\t\n\n").1, Done("", "\t\n\n"));
  }

  #[test]
  fn test_comment_nl() {
    let p = Parser::new();
    assert_eq!(p.comment_nl("\r\n\t#çô₥₥èñƭñèωℓïñè\n \n \n").1, Done("",
      CommentNewLines::new_str(
        "\r\n\t", Comment::new_str("çô₥₥èñƭñèωℓïñè"), "\n \n \n"
      )
    ));
  }

  #[test]
  fn test_comment_or_nl() {
    let mut p = Parser::new();
    assert_eq!(p.comment_or_nl("#ωôřƙωôřƙ\n").1, Done("",
      CommentOrNewLines::Comment(CommentNewLines::new_str(
        "", Comment::new_str("ωôřƙωôřƙ"), "\n"
      ))
    ));
    p = Parser::new();
    assert_eq!(p.comment_or_nl(" \t\n#ωôřƙωôřƙ\n \r\n").1, Done("",
      CommentOrNewLines::Comment(CommentNewLines::new_str(
        " \t\n", Comment::new_str("ωôřƙωôřƙ"), "\n \r\n"
      ))
    ));
    p = Parser::new();
    assert_eq!(p.comment_or_nl("\n\t\r\n ").1, Done("", CommentOrNewLines::NewLines(Str::Str("\n\t\r\n "))));
  }

  #[test]
  fn test_array_value() {
    let mut p = Parser::new();
    p.keychain.borrow_mut().push(Key::Index(Cell::new(0)));
    assert_eq!(p.array_value("54.6, \n#çô₥₥èñƭ\n\n").1,
      Done("",ArrayValue::new(
        Rc::new(RefCell::new(Value::Float(Str::Str("54.6")))), Some(WSSep::new_str("", " ")),
        vec![CommentOrNewLines::Comment(CommentNewLines::new_str(
          "\n", Comment::new_str("çô₥₥èñƭ"), "\n\n"
        ))]
      ))
    );
    p = Parser::new();
    p.keychain.borrow_mut().push(Key::Index(Cell::new(0)));
    assert_eq!(p.array_value("\"ƨƥáϱλèƭƭï\"").1,
      Done("",ArrayValue::new(
        Rc::new(RefCell::new(Value::String(Str::Str("ƨƥáϱλèƭƭï"), StrType::Basic))), None, vec![CommentOrNewLines::NewLines(Str::Str(""))]
      ))
    );
    p = Parser::new();
    p.keychain.borrow_mut().push(Key::Index(Cell::new(0)));
    assert_eq!(p.array_value("44_9 , ").1,
      Done("",ArrayValue::new(
        Rc::new(RefCell::new(Value::Integer(Str::Str("44_9")))), Some(WSSep::new_str(" ", " ")),
        vec![CommentOrNewLines::NewLines(Str::Str(""))]
      ))
    );
  }

  #[test]
  fn test_array_values() {
    let mut p = Parser::new();
    p.keychain.borrow_mut().push(Key::Index(Cell::new(0)));
    assert_eq!(p.array_values("1, 2, 3").1, Done("", vec![
      ArrayValue::new(Rc::new(RefCell::new(Value::Integer(Str::Str("1")))), Some(WSSep::new_str("", " ")),
      vec![CommentOrNewLines::NewLines(Str::Str(""))]),
      ArrayValue::new(Rc::new(RefCell::new(Value::Integer(Str::Str("2")))), Some(WSSep::new_str("", " ")),
      vec![CommentOrNewLines::NewLines(Str::Str(""))]),
      ArrayValue::new(Rc::new(RefCell::new(Value::Integer(Str::Str("3")))), None, vec![CommentOrNewLines::NewLines(Str::Str(""))])
    ]));
    p = Parser::new();
    p.keychain.borrow_mut().push(Key::Index(Cell::new(0)));
    assert_eq!(p.array_values("1, 2, #çô₥₥èñƭ\n3, ").1, Done("", vec![
      ArrayValue::new(Rc::new(RefCell::new(Value::Integer(Str::Str("1")))), Some(WSSep::new_str("", " ")),
      vec![CommentOrNewLines::NewLines(Str::Str(""))]),
      ArrayValue::new(Rc::new(RefCell::new(Value::Integer(Str::Str("2")))), Some(WSSep::new_str("", " ")),
        vec![CommentOrNewLines::Comment(CommentNewLines::new_str("", Comment::new_str("çô₥₥èñƭ"), "\n"))]),
      ArrayValue::new(Rc::new(RefCell::new(Value::Integer(Str::Str("3")))), Some(WSSep::new_str("", " ")),
      vec![CommentOrNewLines::NewLines(Str::Str(""))])
    ]));
  }

  #[test]
  fn test_non_nested_array() {
    let p = Parser::new();
    assert_eq!(p.array("[2010-10-10T10:10:10.33Z, 1950-03-30T21:04:14.123+05:00]").1,
      Done("", Rc::new(RefCell::new(Array::new(
        vec![ArrayValue::new(
          Rc::new(RefCell::new(Value::DateTime(DateTime::new(
            Date::new_str("2010", "10", "10"), Some(Time::new_str("10", "10", "10", Some("33"),
              Some(TimeOffset::Zulu)
          )))))),
          Some(WSSep::new_str("", " ")),
          vec![CommentOrNewLines::NewLines(Str::Str(""))]
        ),
        ArrayValue::new(
          Rc::new(RefCell::new(Value::DateTime(DateTime::new(
            Date::new_str("1950", "03", "30"), Some(Time::new_str("21", "04", "14", Some("123"),
            Some(TimeOffset::Time(TimeOffsetAmount::new_str("+", "05", "00")))
          )))))),
          None, vec![CommentOrNewLines::NewLines(Str::Str(""))]
        )],
        vec![CommentOrNewLines::NewLines(Str::Str(""))], vec![CommentOrNewLines::NewLines(Str::Str(""))]
      ))))
    );
  }

  #[test]
  fn test_nested_array() {
    let p = Parser::new();
    assert_eq!(p.array("[[3,4], [4,5], [6]]").1,
      Done("", Rc::new(RefCell::new(Array::new(
        vec![
          ArrayValue::new(
            Rc::new(RefCell::new(Value::Array(Rc::new(RefCell::new(Array::new(
              vec![
                ArrayValue::new(
                  Rc::new(RefCell::new(Value::Integer(Str::Str("3")))), Some(WSSep::new_str("", "")),
                  vec![CommentOrNewLines::NewLines(Str::Str(""))]
                ),
                ArrayValue::new(
                  Rc::new(RefCell::new(Value::Integer(Str::Str("4")))), None, vec![CommentOrNewLines::NewLines(Str::Str(""))]
                )
              ],
              vec![CommentOrNewLines::NewLines(Str::Str(""))], vec![CommentOrNewLines::NewLines(Str::Str(""))]
            )))))),
            Some(WSSep::new_str("", " ")),
            vec![CommentOrNewLines::NewLines(Str::Str(""))]
          ),
          ArrayValue::new(
            Rc::new(RefCell::new(Value::Array(Rc::new(RefCell::new(Array::new(
              vec![
                ArrayValue::new(
                  Rc::new(RefCell::new(Value::Integer(Str::Str("4")))), Some(WSSep::new_str("", "")),
                  vec![CommentOrNewLines::NewLines(Str::Str(""))]
                ),
                ArrayValue::new(
                    Rc::new(RefCell::new(Value::Integer(Str::Str("5")))), None, vec![CommentOrNewLines::NewLines(Str::Str(""))]
                )
              ],
              vec![CommentOrNewLines::NewLines(Str::Str(""))], vec![CommentOrNewLines::NewLines(Str::Str(""))]
            )))))),
            Some(WSSep::new_str("", " ")),
            vec![CommentOrNewLines::NewLines(Str::Str(""))]
          ),
          ArrayValue::new(
            Rc::new(RefCell::new(Value::Array(Rc::new(RefCell::new(Array::new(
              vec![
                ArrayValue::new(
                  Rc::new(RefCell::new(Value::Integer(Str::Str("6")))), None, vec![CommentOrNewLines::NewLines(Str::Str(""))]
                )
              ],
             vec![CommentOrNewLines::NewLines(Str::Str(""))], vec![CommentOrNewLines::NewLines(Str::Str(""))]
            )))))),
            None, vec![CommentOrNewLines::NewLines(Str::Str(""))]
          )
        ],
        vec![CommentOrNewLines::NewLines(Str::Str(""))], vec![CommentOrNewLines::NewLines(Str::Str(""))]
      ))))
    );
  }

  #[test]
  fn test_table_keyval() {
    let p = Parser::new();
    assert_eq!(p.table_keyval("\"Ì WúƲ Húϱƨ!\"\t=\t'Mè ƭôô!' ").1, Done("", TableKeyVal::new(
      KeyVal::new_str(
        "\"Ì WúƲ Húϱƨ!\"", WSSep::new_str("\t", "\t"), Rc::new(RefCell::new(Value::String(Str::Str("Mè ƭôô!"), StrType::Literal)))
      ),
      WSSep::new_str("", " "),
    )));
  }

  #[test]
  fn test_inline_table_keyvals_non_empty() {
    let p = Parser::new();
    assert_eq!(p.inline_table_keyvals_non_empty(" Key =\t54,\"Key2\" = '34.99'\t").1,
      Done("", vec![
        TableKeyVal::new(
          KeyVal::new_str(
            "Key", WSSep::new_str(" ", "\t"),
            Rc::new(RefCell::new(Value::Integer(Str::Str("54"))))
          ),
          WSSep::new_str(" ", "")
        ),
        TableKeyVal::new(
          KeyVal::new_str(
            "\"Key2\"", WSSep::new_str( " ", " "),
            Rc::new(RefCell::new(Value::String(Str::Str("34.99"), StrType::Literal)))
          ),
          WSSep::new_str("", "\t")
        )
      ])
    );
  }

  #[test]
  fn test_inline_table() {
    let p = Parser::new();
    assert_eq!(p.inline_table("{\tKey = 3.14E+5 , \"Key2\" = '''New\nLine'''\t}").1,
      Done("", Rc::new(RefCell::new(InlineTable::new(
        vec![
          TableKeyVal::new(
            KeyVal::new_str(
              "Key", WSSep::new_str(" ", " "),
              Rc::new(RefCell::new(Value::Float(Str::Str("3.14E+5"))))
            ),
            WSSep::new_str("", " ")
          ),
          TableKeyVal::new(
            KeyVal::new_str("\"Key2\"", WSSep::new_str(" ", " "),
              Rc::new(RefCell::new(Value::String(Str::Str("New\nLine"), StrType::MLLiteral)))
            ),
            WSSep::new_str(" ", "\t")
          )
        ],
        WSSep::new_str("\t", "")
      ))))
    );
  }
}
