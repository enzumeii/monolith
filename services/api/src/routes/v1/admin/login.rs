// // Auth Guard Middleware
// pub async fn auth_middleware(
//     AuthBearer(token): AuthBearer,
//     mut req: Request,
//     next: Next,
// ) -> Result<Response, StatusCode> {
//     let token_data = decode::<Claims>(
//         &token,
//         &DecodingKey::from_secret(JWT_SECRET),
//         &Validation::default(),
//     )
//     .map_err(|_| StatusCode::UNAUTHORIZED)?;
//
//     req.extensions_mut().insert(token_data.claims);
//     Ok(next.run(req).await)
// }
