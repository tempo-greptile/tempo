//! Heap profiling support via jemalloc pprof.
//!
//! When enabled, exposes `/debug/pprof/heap` endpoint that returns a gzipped
//! pprof-format heap profile suitable for analysis with Polar Signals or pprof tools.

use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode, header},
    routing::get,
};

pub(crate) fn router() -> Router {
    Router::new().route("/debug/pprof/heap", get(heap_profile_handler))
}

async fn heap_profile_handler() -> Response<Body> {
    let Some(prof_ctl) = jemalloc_pprof::PROF_CTL.as_ref() else {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("heap profiling not enabled"))
            .expect("failed to create response");
    };

    let mut prof_ctl = prof_ctl.lock().await;

    if !prof_ctl.activated() {
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::from("heap profiling not activated"))
            .expect("failed to create response");
    }

    match prof_ctl.dump_pprof() {
        Ok(pprof) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_ENCODING, "gzip")
            .header(
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"heap.pb.gz\"",
            )
            .body(Body::from(pprof))
            .expect("failed to create response"),
        Err(err) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("failed to dump heap profile: {err}")))
            .expect("failed to create response"),
    }
}
