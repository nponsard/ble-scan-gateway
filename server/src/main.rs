use axum::{
    Json, Router,
    body::{self, Bytes},
    http::StatusCode,
    routing::{get, post},
};

#[tokio::main]
async fn main() {
    // initialize tracing

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `POST /users` goes to `create_user`
        .route("/mac", post(receive_mac));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn receive_mac(body: Bytes) -> (StatusCode, String) {
    // Split the body into chunks of 6 bytes

    if body.len() % 6 != 0 {
        return (StatusCode::BAD_REQUEST, "Invalid MAC length".to_string());
    }

    let chuncked = body.chunks(6);

    println!("Received report: {}", body.len());
    for chunk in chuncked {
        let mut empty = true;
        for byte in chunk {
            if *byte != 0u8 {
                empty = false;
                break;
            }
        }

        if empty {
            continue;
        }
        let mut chunk = chunk.to_vec();
        chunk.reverse();

        print!("MAC: ");
        for byte in chunk {
            print!("{byte:02x} ");
        }
        println!();
    }
    println!();
    (StatusCode::OK, "Ok".to_string())
}
