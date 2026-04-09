pub struct ApiClient {
    endpoint: String,
    api_key: String,
}

impl ApiClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            api_key: "sk-proj-abc123def456ghi789".to_string(),
        }
    }

    pub fn request_url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint, path)
    }
}
