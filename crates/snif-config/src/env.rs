pub mod keys {
    pub const SNIF_API_KEY: &str = "SNIF_API_KEY";
    pub const OPENAI_API_KEY: &str = "OPENAI_API_KEY";
    pub const GITHUB_TOKEN: &str = "GITHUB_TOKEN";
    pub const GITLAB_TOKEN: &str = "GITLAB_TOKEN";
    pub const BRAINTRUST_API_KEY: &str = "BRAINTRUST_API_KEY";
    pub const CI_JOB_TOKEN: &str = "CI_JOB_TOKEN";
}

pub mod app {
    pub const SNIF_APP_ID: &str = "SNIF_APP_ID";
    pub const SNIF_APP_PRIVATE_KEY: &str = "SNIF_APP_PRIVATE_KEY";
    pub const SNIF_APP_INSTALLATION_ID: &str = "SNIF_APP_INSTALLATION_ID";
    pub const SNIF_ENDPOINT: &str = "SNIF_ENDPOINT";
    pub const SNIF_DB_PATH: &str = "SNIF_DB_PATH";
    pub const SNIF_PLATFORM: &str = "SNIF_PLATFORM";
    pub const SNIF_PR_NUMBER: &str = "SNIF_PR_NUMBER";
    pub const SNIF_BRAINTRUST_PROJECT_ID: &str = "SNIF_BRAINTRUST_PROJECT_ID";
    pub const SNIF_REVIEW_MODEL: &str = "SNIF_REVIEW_MODEL";
    pub const SNIF_SUMMARY_MODEL: &str = "SNIF_SUMMARY_MODEL";
}

pub mod ci {
    pub const CI: &str = "CI";
    pub const GITHUB_ACTIONS: &str = "GITHUB_ACTIONS";
    pub const GITLAB_CI: &str = "GITLAB_CI";
    pub const GITHUB_REPOSITORY: &str = "GITHUB_REPOSITORY";
    pub const GITHUB_PR_NUMBER: &str = "GITHUB_PR_NUMBER";
    pub const GITHUB_REF_NAME: &str = "GITHUB_REF_NAME";
    pub const CI_PROJECT_PATH: &str = "CI_PROJECT_PATH";
    pub const CI_MERGE_REQUEST_IID: &str = "CI_MERGE_REQUEST_IID";
    pub const CI_API_V4_URL: &str = "CI_API_V4_URL";
    pub const CI_COMMIT_REF_NAME: &str = "CI_COMMIT_REF_NAME";
}
