# Rustnish
Experimental project to learn Rust. A reverse proxy.

## Goal 1: Just pipe HTTP requests through
Completed: yes

A webserver like Apache is listening on port 80. Write a reverse proxy service
that does nothing but forwarding HTTP requests to port 80. The service must
listen on port 9090. The service must not modify the HTTP response in any way.
