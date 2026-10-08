//! Read-only API transport. Credentials only in sensitive headers to an exact HTTPS origin.
use crate::{filter::{safe_api_url, safe_web_url}, model::{MAX_BODY_BYTES, MAX_PAGES, Repo, ThreadTask, parse_time}};
use chrono::{DateTime, Utc};
use reqwest::{blocking::{Client, Response}, header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT}, redirect::Policy};
use serde_json::{Value, json};
use std::{cell::{Cell, RefCell}, collections::HashSet, io::Read, time::Duration};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("GitHub-Token ungültig oder abgelaufen. Bitte neu verbinden.")] Unauthorized,
    #[error("GitHub verweigert den Zugriff (HTTP {0}). Private Repositories brauchen einen passenden Nur-Lesen-Detail-Token; Organisationen eventuell eine Freigabe.")] Access(u16),
    #[error("GitHub-Abfragelimit erreicht. Automatischer neuer Versuch nach der Wartezeit.")] RateLimit(i64),
    #[error("Das Abfragebudget dieses Durchlaufs ist aufgebraucht. Verbleibende Threads werden später verarbeitet.")] Budget,
    #[error("Netzwerkfehler oder Zeitüberschreitung beim Kontakt mit GitHub.")] Network,
    #[error("GitHub lieferte eine unerwartete Antwort (HTTP {0}).")] Http(u16),
    #[error("Antwort zu groß oder ungültig. Sync-Zeitpunkt bleibt unverändert.")] Invalid,
    #[error("Seitengrenze erreicht: Abfrage wurde nicht als vollständig bestätigt.")] Pagination,
    #[error("Unerlaubtes API-Ziel wurde blockiert.")] Origin,
}
pub type ApiResult<T> = Result<T, ApiError>;
#[derive(Debug, Default, Clone)]
pub struct Rate { pub remaining: Option<u32>, pub reset: Option<i64>, pub poll_floor: u64 }
pub struct Api {
    primary: Client,
    detail: Option<Client>,
    pub calls: Cell<u32>,
    pub rate: RefCell<Rate>,
}

fn client(token: &str) -> anyhow::Result<Client> {
    anyhow::ensure!(!token.is_empty() && token.len() < 1024 && !token.chars().any(char::is_whitespace), "Ungültiges Token-Format.");
    let mut headers=HeaderMap::new();
    let mut auth=HeaderValue::from_str(&format!("Bearer {token}"))?;
    auth.set_sensitive(true);
    headers.insert(AUTHORIZATION,auth);
    headers.insert(ACCEPT,HeaderValue::from_static("application/vnd.github+json"));
    headers.insert(USER_AGENT,HeaderValue::from_static("Hush/0.1.0 (+local-desktop)"));
    headers.insert("X-GitHub-Api-Version",HeaderValue::from_static("2022-11-28"));
    Ok(Client::builder().default_headers(headers).https_only(true).redirect(Policy::none())
        .connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(20))
        .no_proxy().build()?)
}
impl Api {
    pub fn new(primary: &str, detail: Option<&str>) -> anyhow::Result<Self> {
        Ok(Self { primary:client(primary)?,detail:detail.map(client).transpose()?,calls:Cell::new(0),rate:RefCell::new(Rate { poll_floor:60,..Default::default() }) })
    }
    pub fn reset_cycle(&self) { self.calls.set(0); }
    fn request(&self, primary: bool, raw_url: &str, body: Option<&Value>) -> ApiResult<(Value, HeaderMap)> {
        let url=safe_api_url(raw_url).map_err(|_| ApiError::Origin)?;
        if self.calls.get() >= 160 { return Err(ApiError::Budget); }
        self.calls.set(self.calls.get()+1);
        let client=if primary { &self.primary } else { self.detail.as_ref().unwrap_or(&self.primary) };
        let request=if let Some(body)=body {
            if url.path()!="/graphql" { return Err(ApiError::Origin); }
            client.post(url).json(body)
        } else { client.get(url) };
        let response=request.send().map_err(|_| ApiError::Network)?;
        self.decode(response)
    }
    fn decode(&self, response: Response) -> ApiResult<(Value, HeaderMap)> {
        let headers=response.headers().clone();
        let status=response.status().as_u16();
        let number=|name: &str| headers.get(name).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<i64>().ok());
        let remaining=number("x-ratelimit-remaining");
        let reset=number("x-ratelimit-reset");
        {
            let mut rate=self.rate.borrow_mut();
            if let Some(n)=remaining { rate.remaining=Some(n.max(0) as u32); }
            if reset.is_some() { rate.reset=reset; }
            if let Some(n)=number("x-poll-interval") { rate.poll_floor=n.max(60) as u64; }
        }
        if status==429 || (status==403 && (remaining==Some(0) || number("retry-after").is_some())) {
            let until=number("retry-after").map(|s| Utc::now().timestamp()+s)
                .or(reset).unwrap_or(Utc::now().timestamp()+300);
            return Err(ApiError::RateLimit(until));
        }
        if status==401 { return Err(ApiError::Unauthorized); }
        if status==403 || status==404 { return Err(ApiError::Access(status)); }
        // Redirects are deliberately not followed, including same-origin redirects.
        if !(200..300).contains(&status) { return Err(ApiError::Http(status)); }
        if response.content_length().is_some_and(|n| n>MAX_BODY_BYTES as u64) { return Err(ApiError::Invalid); }
        let mut bytes=Vec::new();
        response.take(MAX_BODY_BYTES as u64+1).read_to_end(&mut bytes).map_err(|_| ApiError::Network)?;
        if bytes.len()>MAX_BODY_BYTES { return Err(ApiError::Invalid); }
        let value=serde_json::from_slice(&bytes).map_err(|_| ApiError::Invalid)?;
        Ok((value,headers))
    }
    pub fn get(&self, path: &str) -> ApiResult<Value> {
        let full=format!("https://api.github.com{path}");
        match self.request(false,&full,None) {
            // A restricted detail token may not cover a public repository. The minimal classic
            // token can still read public resources. Both credentials remain on the same origin.
            Err(ApiError::Access(_)) if self.detail.is_some() => self.request(true,&full,None).map(|r|r.0),
            result => result.map(|r|r.0),
        }
    }
    fn page_size(path: &str) -> usize { if path == "/notifications" { 50 } else { 100 } }
    fn page_url(path: &str, query: &[(&str,String)], page: usize) -> ApiResult<String> {
        let mut url=safe_api_url(&format!("https://api.github.com{path}")).map_err(|_|ApiError::Origin)?;
        { let mut pairs=url.query_pairs_mut(); for (key,value) in query { pairs.append_pair(key,value); } pairs.append_pair("per_page",&Self::page_size(path).to_string()).append_pair("page",&page.to_string()); }
        Ok(url.to_string())
    }
    pub fn list(&self, path: &str, query: &[(&str,String)], primary: bool) -> ApiResult<Vec<Value>> {
        let mut result=Vec::new();
        for page in 1..=MAX_PAGES {
            let url=Self::page_url(path,query,page)?;
            let response=match self.request(primary,&url,None) {
                Err(ApiError::Access(_)) if !primary && self.detail.is_some() => self.request(true,&url,None),
                other => other,
            }?;
            let items=response.0.as_array().ok_or(ApiError::Invalid)?;
            let len=items.len(); result.extend(items.iter().cloned());
            if len<Self::page_size(path) { return Ok(result); }
        }
        Err(ApiError::Pagination)
    }
    pub fn identity(&self, primary: bool) -> ApiResult<(String,Vec<String>)> {
        let (value,headers)=self.request(primary,"https://api.github.com/user",None)?;
        let login=value["login"].as_str().filter(|s| !s.is_empty()).ok_or(ApiError::Invalid)?.to_owned();
        let scopes=headers.get("x-oauth-scopes").and_then(|v|v.to_str().ok()).unwrap_or("")
            .split(',').map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned).collect();
        Ok((login,scopes))
    }
    pub fn teams(&self) -> ApiResult<HashSet<String>> {
        let mut teams=HashSet::new();
        for t in self.list("/user/teams",&[],true)? {
            if let (Some(org),Some(slug))=(t["organization"]["login"].as_str(),t["slug"].as_str()) {
                if let Ok(team)=Repo::parse(&format!("{org}/{slug}")) { teams.insert(team.to_string()); }
            }
        }
        Ok(teams)
    }
    pub fn notification_tasks(&self, since: DateTime<Utc>, quiet: bool) -> ApiResult<Vec<ThreadTask>> {
        let items=self.list("/notifications",&[("all","true".into()),("since",since.to_rfc3339())],true)?;
        let mut tasks=Vec::new();
        for n in items {
            let Some(full_name)=n["repository"]["full_name"].as_str() else { continue; };
            let repo=Repo::parse(full_name).map_err(|_|ApiError::Invalid)?;
            let subject=&n["subject"];
            let subject_url=subject["url"].as_str().unwrap_or("").to_owned();
            let kind=subject["type"].as_str().unwrap_or("").to_owned();
            // Unsupported kinds are explicitly surfaced by the engine, never emitted as mentions.
            let id=format!("{}:{}",repo,if subject_url.is_empty() { n["id"].as_str().unwrap_or("unknown") } else { &subject_url });
            tasks.push(ThreadTask { id,repository:repo,subject_type:kind,title:subject["title"].as_str().unwrap_or("GitHub-Aktivität").to_owned(),
                subject_url,latest_comment_url:subject["latest_comment_url"].as_str().map(str::to_owned),
                reason:n["reason"].as_str().unwrap_or("").to_owned(),since,quiet });
        }
        Ok(tasks)
    }
    /// Independent discovery for reviews on authored PRs (including recently closed PRs).
    pub fn authored_pr_tasks(&self, login: &str, since: DateTime<Utc>, quiet: bool) -> ApiResult<Vec<ThreadTask>> {
        let query=format!("is:pr author:{login} updated:>={}",since.format("%Y-%m-%d"));
        let mut tasks=Vec::new();
        for page in 1..=MAX_PAGES {
            let url=Self::page_url("/search/issues",&[("q",query.clone()),("sort","updated".into()),("order","desc".into())],page)?;
            let (v,_)=self.request(false,&url,None)?;
            if v["incomplete_results"].as_bool()==Some(true) { return Err(ApiError::Pagination); }
            let items=v["items"].as_array().ok_or(ApiError::Invalid)?;
            for item in items {
                let raw=item["repository_url"].as_str().ok_or(ApiError::Invalid)?;
                let parsed=safe_api_url(raw).map_err(|_|ApiError::Origin)?;
                let slug=parsed.path().strip_prefix("/repos/").ok_or(ApiError::Invalid)?;
                let repo=Repo::parse(slug).map_err(|_|ApiError::Invalid)?;
                let number=item["number"].as_u64().ok_or(ApiError::Invalid)?;
                let subject_url=format!("https://api.github.com{}/pulls/{number}",repo.api_path());
                tasks.push(ThreadTask { id:format!("{repo}:{subject_url}"),repository:repo,subject_type:"PullRequest".into(),title:item["title"].as_str().unwrap_or("Pull Request").into(),subject_url,latest_comment_url:None,reason:"author".into(),since,quiet });
            }
            if items.len()<100 { return Ok(tasks); }
        }
        Err(ApiError::Pagination)
    }
    pub fn new_issues(&self, repo: &Repo, since: DateTime<Utc>) -> ApiResult<Vec<Value>> {
        let path=format!("{}/issues",repo.api_path());
        let query=[("state","all".into()),("sort","created".into()),("direction","desc".into()),("since",since.to_rfc3339())];
        let mut result=Vec::new();
        for page in 1..=MAX_PAGES {
            let url=Self::page_url(&path,&query,page)?;
            let response=match self.request(false,&url,None) {
                Err(ApiError::Access(_)) if self.detail.is_some()=>self.request(true,&url,None),
                other=>other,
            }?;
            let items=response.0.as_array().ok_or(ApiError::Invalid)?;
            let mut older=false;
            for item in items {
                if parse_time(item["created_at"].as_str().unwrap_or("")).is_some_and(|at|at<since) { older=true; break; }
                result.push(item.clone());
            }
            if older || items.len()<100 { return Ok(result); }
        }
        Err(ApiError::Pagination)
    }
    fn graphql(&self, query: &str, variables: Value) -> ApiResult<Value> {
        let (value,_)=self.request(false,"https://api.github.com/graphql",Some(&json!({"query":query,"variables":variables})))?;
        if value.get("errors").is_some() { return Err(ApiError::Access(403)); }
        value.get("data").cloned().ok_or(ApiError::Invalid)
    }
    /// Paginated Discussion comments AND replies. No silent "only latest comment" shortcut.
    pub fn discussion_comments(&self, web_url: &str) -> ApiResult<Vec<Value>> {
        safe_web_url(web_url).map_err(|_|ApiError::Origin)?;
        const QUERY:&str=r#"query($url:URI!,$after:String){resource(url:$url){... on Discussion {comments(first:100,after:$after){nodes{id body createdAt updatedAt url author{login} replies(first:100){nodes{id body createdAt updatedAt url author{login}} pageInfo{hasNextPage endCursor}}} pageInfo{hasNextPage endCursor}}}}}"#;
        const REPLIES:&str=r#"query($id:ID!,$after:String){node(id:$id){... on DiscussionComment {replies(first:100,after:$after){nodes{id body createdAt updatedAt url author{login}} pageInfo{hasNextPage endCursor}}}}}"#;
        let convert=|v:&Value| json!({"id":v["id"],"body":v["body"],"created_at":v["createdAt"],"updated_at":v["updatedAt"],"html_url":v["url"],"user":v["author"]});
        let mut result=Vec::new(); let mut after:Option<String>=None;
        for _ in 0..MAX_PAGES {
            let data=self.graphql(QUERY,json!({"url":web_url,"after":after}))?;
            let connection=&data["resource"]["comments"];
            let nodes=connection["nodes"].as_array().ok_or(ApiError::Invalid)?;
            for node in nodes {
                result.push(convert(node));
                let replies=&node["replies"];
                if let Some(items)=replies["nodes"].as_array() { result.extend(items.iter().map(&convert)); }
                let mut reply_next=replies["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
                let mut cursor=replies["pageInfo"]["endCursor"].as_str().map(str::to_owned);
                let mut pages=1;
                while reply_next {
                    if pages>=MAX_PAGES { return Err(ApiError::Pagination); }
                    let d=self.graphql(REPLIES,json!({"id":node["id"],"after":cursor}))?;
                    let c=&d["node"]["replies"];
                    let items=c["nodes"].as_array().ok_or(ApiError::Invalid)?;
                    result.extend(items.iter().map(&convert));
                    reply_next=c["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
                    cursor=c["pageInfo"]["endCursor"].as_str().map(str::to_owned); pages+=1;
                }
            }
            if connection["pageInfo"]["hasNextPage"].as_bool()!=Some(true) { return Ok(result); }
            after=connection["pageInfo"]["endCursor"].as_str().map(str::to_owned);
        }
        Err(ApiError::Pagination)
    }
}

pub fn validate_scopes(scopes: &[String]) -> anyhow::Result<bool> {
    anyhow::ensure!(scopes.iter().any(|s|s=="notifications"), "Der klassische Token braucht den Scope notifications.");
    let forbidden=scopes.iter().any(|s| !["notifications", "read:org"].contains(&s.as_str()));
    anyhow::ensure!(!forbidden,"Dieser Token ist zu weit berechtigt. Bitte einen separaten klassischen Token nur mit notifications und optional read:org erstellen.");
    Ok(scopes.iter().any(|s|s=="read:org"))
}

#[cfg(test)]
mod paging_tests {
    use super::*;
    #[test]
    fn notifications_obey_fifty_item_page_limit() {
        assert_eq!(Api::page_size("/notifications"), 50);
        let url = url::Url::parse(&Api::page_url("/notifications", &[], 2).unwrap()).unwrap();
        assert!(url.query_pairs().any(|(k,v)| k=="per_page" && v=="50"));
        assert!(url.query_pairs().any(|(k,v)| k=="page" && v=="2"));
        assert_eq!(Api::page_size("/repos/a/b/issues"), 100);
    }
}
