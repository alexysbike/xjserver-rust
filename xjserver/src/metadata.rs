use std::collections::HashMap;

/// Incoming (lowercase keys) and outgoing response headers.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    incoming: HashMap<String, String>,
    outgoing: HashMap<String, String>,
}

impl Metadata {
    pub fn new(incoming: HashMap<String, String>) -> Self {
        Self {
            incoming,
            outgoing: HashMap::new(),
        }
    }

    pub fn from_header_iter<I, K, V>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let incoming = iter
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_ascii_lowercase(), v.as_ref().to_string()))
            .collect();
        Self::new(incoming)
    }

    pub fn get_incoming(&self, name: &str) -> Option<&str> {
        self.incoming
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn incoming(&self) -> &HashMap<String, String> {
        &self.incoming
    }

    pub fn set_outgoing(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.outgoing
            .insert(name.into().to_ascii_lowercase(), value.into());
    }

    pub fn outgoing(&self) -> &HashMap<String, String> {
        &self.outgoing
    }

    pub fn has_outgoing(&self) -> bool {
        !self.outgoing.is_empty()
    }
}
