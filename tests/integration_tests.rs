extern crate rustnish;

#[test]
fn it_works() {
    let port: u16 = 9090;
    // How can I call the function from main.rs?
    let mut listening = rustnish::start_server(port);
    let _guard = listening.close();
}
