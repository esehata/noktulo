use crate::user::post::{PostKind, SignedPost};

pub struct Timeline {
    posts: Vec<SignedPost>,
}

impl Timeline {
    pub fn new() -> Timeline {
        Timeline { posts: Vec::new() }
    }

    pub fn push(&mut self, sigpost: SignedPost) {
        match sigpost.post.content {
            PostKind::Delete(_) => (),
            _ => {
                println!("{}", sigpost);
                self.posts.push(sigpost);
            }
        }
    }

    pub fn get_by_id(&self, id: u128) -> Option<SignedPost> {
        let i = self
            .posts
            .iter()
            .position(|sigpost| sigpost.post.id == id)?;
        Some(self.posts[i].clone())
    }

    pub fn get(&self, index: usize) -> Option<&SignedPost> {
        self.posts.get(self.posts.len() - index - 1)
    }
}
