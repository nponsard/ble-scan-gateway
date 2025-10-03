use axum::{
    Router, extract,
    http::StatusCode,
    routing::{get, post},
};
use common_types::GatewayUpdate;

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
    let listener = tokio::net::TcpListener::bind("0.0.0.0:4230").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn receive_mac(extract::Json(payload): extract::Json<GatewayUpdate>) -> (StatusCode, String) {
    println!("Received update");
    println!(
        "Time of fix: {:?}",
        payload.location.map(|loc| loc.timestamp)
    );
    println!(
        "Position: {:?}",
        payload.location.map(|loc| (loc.latitude, loc.longitude))
    );
    println!("Altitude: {:?}", payload.location.map(|loc| loc.altitude));
    println!("Number of MAC addresses: {}", payload.seen.len());

    for seen in &payload.seen {
        for byte in seen {
            print!("{:02X}:", byte);
        }
        println!();
    }

    (StatusCode::OK, "Ok".to_string())
}
