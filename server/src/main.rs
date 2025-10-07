use axum::{
    Router, extract,
    http::StatusCode,
    routing::{get, post},
};
use chrono::Local;
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
    println!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn receive_mac(extract::Json(payload): extract::Json<GatewayUpdate>) -> (StatusCode, String) {
    let local = Local::now();
    println!("Received update at {:?}", local);
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

    // If there are too many addresses, print them all on one line
    if payload.seen.len() > 40 {
        println!(
            "{}",
            payload
                .seen
                .iter()
                .map(|seen| seen.map(|s| s.to_string()).join(":"))
                .collect::<Vec<String>>()
                .join(", ")
        );
    } else {
        for seen in &payload.seen {
            println!("{}", seen.map(|s| s.to_string()).join(":"));
        }
    }

    (StatusCode::OK, "Ok".to_string())
}
