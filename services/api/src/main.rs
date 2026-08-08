//! API backend — the monolith's pure backend webserver. Hosts the REST/CRUD
//! layer (own data wired in later) and is the ONLY place third-party credentials
//! live (Shopify Admin API, Stripe, future integrations) as proxy modules under
//! src/integrations/. No local DB yet — storage lands on the roadmap later.
//! Never serve the public UI from here.

fn main() {
    println!("api: hello world stub");
}