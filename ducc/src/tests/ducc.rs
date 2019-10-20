use ducc::{Ducc, ExecSettings};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use value::Value;

#[test]
#[should_panic]
fn value_cross_contamination() {
    let ducc_1 = Ducc::new();
    let str_1 = ducc_1.create_string("123").unwrap();
    let ducc_2 = Ducc::new();
    let _str_2 = ducc_2.create_string("456").unwrap();
    let _ = ducc_2.coerce_number(Value::String(str_1));
}

#[test]
fn timeout() {
    let ducc = Ducc::new();
    let start = Instant::now();
    let cancel_fn = move || Instant::now().duration_since(start) > Duration::from_millis(500);
    let settings = ExecSettings { cancel_fn: Some(Box::new(cancel_fn)) };
    let result: Result<(), _> = ducc.exec("for (;;) {}", None, settings);
    assert!(result.is_err());
}

#[test]
fn no_duktape_global() {
    let ducc = Ducc::new();
    let globals = ducc.globals();
    assert!(!globals.contains_key("Duktape").unwrap());
}

#[test]
fn thread_data_sharing() {
    let ducc = Ducc::new();
    let a = ducc.create_string("abc").unwrap();

    let thread_func = |thread: &Ducc| -> String {
        let b = thread.create_string("def").unwrap();
        let this = thread.create_object();
        this.set("a", a.clone()).unwrap();
        this.set("b", b).unwrap();
        let func = thread.compile("this.a + this.b", None).unwrap();
        let value = func.call_method(this, ()).unwrap();
        let result = ducc.coerce_string(value).unwrap();
        result.to_string().unwrap()
    };
    let thread_1_result: String = ducc.with_new_thread(&thread_func);
    assert_eq!(thread_1_result, "abcdef");
    let thread_2_result = ducc.with_new_thread_with_new_global_env(&thread_func);
    assert_eq!(thread_2_result, "abcdef");
}

#[test]
fn thread_globals() {
    let ducc = Ducc::new();
    ducc.globals().set("foo", "foo").unwrap();

    let o = ducc.create_object();
    o.set("a", "a").unwrap();
    ducc.globals().set("o", o.clone()).unwrap();

    let thread_1_result = ducc.with_new_thread(|thread| {
        thread.globals().set("bar", "bar").unwrap();
        ducc.globals().set("baz", "baz").unwrap();
        let thread_1_value = thread.exec("
            bar += 'x';
            o.b = 'b';
            foo + ' ' + bar + ' ' + baz + ' ' + o.a + ' ' + o.b
        ", None, Default::default()).unwrap();
        ducc.coerce_string(thread_1_value).unwrap().to_string().unwrap()
    });
    assert_eq!(thread_1_result, "foo barx baz a b");

    let after_thread_1_value = ducc.exec("
        foo + ' ' + bar + ' ' + baz + ' ' + o.a + ' ' + o.b
    ", None, Default::default()).unwrap();
    let after_thread_1_result = ducc.coerce_string(after_thread_1_value).unwrap().to_string().unwrap();
    assert_eq!(after_thread_1_result, "foo barx baz a b");

    let thread_2_result = ducc.with_new_thread_with_new_global_env(|thread| {
        thread.globals().set("fizz", "fizz").unwrap();
        ducc.globals().set("buzz", "buzz").unwrap();
        thread.globals().set("o", o.clone()).unwrap();
        let thread_2_value = thread.exec("
            o.c = 'c';
            typeof foo + ' ' + typeof bar + ' ' + typeof baz + ' ' + fizz + ' ' + typeof buzz + ' ' + o.a + ' ' + o.b + ' ' + o.c
        ", None, Default::default()).unwrap();
        ducc.coerce_string(thread_2_value).unwrap().to_string().unwrap()
    });
    assert_eq!(thread_2_result, "undefined undefined undefined fizz undefined a b c");

    let after_thread_2_value = ducc.exec("
        foo + ' ' + bar + ' ' + baz + ' ' + typeof fizz + ' ' + buzz + ' ' + o.a + ' ' + o.b + ' ' + o.c
    ", None, Default::default()).unwrap();
    let after_thread_2_result = ducc.coerce_string(after_thread_2_value).unwrap().to_string().unwrap();
    assert_eq!(after_thread_2_result, "foo barx baz undefined buzz a b c");
}

#[test]
fn user_data_drop() {
    let mut ducc = Ducc::new();
    let (count, data) = make_test_user_data();
    ducc.set_user_data("data", data);
    drop(ducc);
    assert_eq!(*count.borrow(), 1000);
}

#[test]
fn user_data_get() {
    let mut ducc = Ducc::new();
    let (_, data) = make_test_user_data();
    ducc.set_user_data("data", data);
    assert!(ducc.get_user_data::<TestUserData>("no-exist").is_none());
    assert!(ducc.get_user_data::<usize>("data").is_none());

    {
        let data = ducc.get_user_data::<TestUserData>("data").unwrap();
        assert_eq!(data.get(), 0);
        data.increase();
        assert_eq!(data.get(), 1);
    }
}

#[test]
fn user_data_remove() {
    let mut ducc = Ducc::new();
    let (count, data) = make_test_user_data();
    ducc.set_user_data("data", data);
    assert_eq!(*count.borrow(), 0);
    let data = ducc.remove_user_data("data").unwrap();
    assert_eq!(*count.borrow(), 0);
    data.downcast_ref::<TestUserData>().unwrap().increase();
    assert_eq!(*count.borrow(), 1);
    drop(data);
    assert_eq!(*count.borrow(), 1000);
}

struct TestUserData {
    count: Rc<RefCell<usize>>,
}

impl TestUserData {
    fn increase(&self) {
        *self.count.borrow_mut() += 1;
    }

    fn get(&self) -> usize {
        *self.count.borrow()
    }
}

impl Drop for TestUserData {
    fn drop(&mut self) {
        *self.count.borrow_mut() = 1000;
    }
}

fn make_test_user_data() -> (Rc<RefCell<usize>>, TestUserData) {
    let count = Rc::new(RefCell::new(0));
    (count.clone(), TestUserData { count })
}
