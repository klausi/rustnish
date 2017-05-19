extern crate rustnish;

#[test]
fn test_pass_through() {
    let port: u16 = 9090;
    let mut listening = rustnish::start_server(port);
    let _guard = listening.close();
}
