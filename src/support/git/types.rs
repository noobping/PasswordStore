#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreGitHead {
    Branch(String),
    UnbornBranch(String),
    Detached,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitRepositoryStatus {
    pub has_repository: bool,
    pub head: StoreGitHead,
    pub dirty: bool,
    pub has_outgoing_commits: bool,
    pub has_incoming_commits: bool,
    pub remotes: Vec<GitRemote>,
}
