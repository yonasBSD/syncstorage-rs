use std::fmt::Display;
use std::future::Future;

use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse},
    http::header::{self, HeaderMap},
};

use syncserver_common::{X_LAST_MODIFIED, X_WEAVE_TIMESTAMP};
use syncstorage_db::SyncTimestamp;

use crate::error::{ApiError, ApiErrorKind};
use crate::web::DOCKER_FLOW_ENDPOINTS;

/// Middleware to set the X-Weave-Timestamp header on all responses.
pub fn set_weave_timestamp<B>(
    request: ServiceRequest,
    service: &impl Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
) -> impl Future<Output = Result<ServiceResponse<B>, actix_web::Error>> {
    let request_path = request.uri().path().to_lowercase();
    let ts = SyncTimestamp::default().as_seconds();
    let fut = service.call(request);

    async move {
        if DOCKER_FLOW_ENDPOINTS.contains(&request_path.as_str()) {
            return fut.await;
        }

        let mut resp = fut.await?;
        insert_weave_timestamp_into_headers(resp.headers_mut(), ts)?;
        Ok(resp)
    }
}

/// Set a X-Weave-Timestamp header on all responses (depending on the
/// response's X-Last-Modified header)
fn insert_weave_timestamp_into_headers(headers: &mut HeaderMap, ts: f64) -> Result<(), ApiError> {
    fn invalid_xlm<E>(e: E) -> ApiError
    where
        E: Display,
    {
        ApiErrorKind::Internal(format!("Invalid X-Last-Modified response header: {}", e)).into()
    }

    let weave_ts = if let Some(val) = headers.get(X_LAST_MODIFIED) {
        let resp_ts = val
            .to_str()
            .map_err(invalid_xlm)?
            .parse::<f64>()
            .map_err(invalid_xlm)?;
        if resp_ts > ts {
            resp_ts
        } else {
            ts
        }
    } else {
        ts
    };
    headers.insert(
        header::HeaderName::from_static(X_WEAVE_TIMESTAMP),
        header::HeaderValue::from_str(&format!("{:.2}", &weave_ts)).map_err(invalid_xlm)?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http, HttpResponse};
    use chrono::Utc;

    #[test]
    fn test_no_modified_header() {
        let mut resp = HttpResponse::build(http::StatusCode::OK).finish();
        insert_weave_timestamp_into_headers(
            resp.headers_mut(),
            SyncTimestamp::default().as_seconds(),
        )
        .unwrap();
        let weave_hdr = resp
            .headers()
            .get(X_WEAVE_TIMESTAMP)
            .unwrap()
            .to_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let uts = Utc::now().timestamp_millis() as u64;
        let weave_hdr = (weave_hdr * 1000.0) as u64;
        // Add 10 to compensate for how fast Rust can run these
        // tests (Due to 2-digit rounding for the sync ts).
        assert!(weave_hdr < uts + 10);
        assert!(weave_hdr > uts - 2000);
    }

    #[test]
    fn test_older_timestamp() {
        let ts = (Utc::now().timestamp_millis() as u64) - 1000;
        let hts = format!("{:.*}", 2, ts as f64 / 1_000.0);
        let mut resp = HttpResponse::build(http::StatusCode::OK)
            .insert_header((X_LAST_MODIFIED, hts.clone()))
            .finish();
        insert_weave_timestamp_into_headers(resp.headers_mut(), ts as f64).unwrap();
        let weave_hdr = resp
            .headers()
            .get(X_WEAVE_TIMESTAMP)
            .unwrap()
            .to_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let hts = hts.parse::<f64>().unwrap();
        assert!(weave_hdr > hts);
    }

    #[test]
    fn test_newer_timestamp() {
        let ts = (Utc::now().timestamp_millis() as u64) + 4000;
        let hts = format!("{:.2}", ts as f64 / 1_000.0);
        let mut resp = HttpResponse::build(http::StatusCode::OK)
            .insert_header((X_LAST_MODIFIED, hts.clone()))
            .finish();
        insert_weave_timestamp_into_headers(resp.headers_mut(), ts as f64 / 1_000.0).unwrap();
        let weave_hdr = resp
            .headers()
            .get(X_WEAVE_TIMESTAMP)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(weave_hdr, hts);
    }
}
