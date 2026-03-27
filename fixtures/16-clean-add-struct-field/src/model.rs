#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub email: String,
    pub role: String,
    pub avatar_url: Option<String>,
}

impl Default for User {
    fn default() -> Self {
        Self {
            name: String::new(),
            email: String::new(),
            role: "viewer".to_string(),
            avatar_url: None,
        }
    }
}
