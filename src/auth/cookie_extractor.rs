// Cookie extraction from real browsers using rookie crate
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Supported browsers for cookie extraction
#[derive(Debug, Clone, Copy)]
pub enum Browser {
    Chrome,
    Firefox,
    Edge,
    Safari,
    Opera,
    OperaGX,
    Brave,
    Chromium,
    Vivaldi,
}

impl Browser {
    /// Get all supported browsers in priority order
    pub fn all() -> Vec<Browser> {
        vec![
            Browser::Chrome,
            Browser::Edge,
            Browser::Brave,
            Browser::Opera,
            Browser::OperaGX,
            Browser::Firefox,
            Browser::Chromium,
            Browser::Vivaldi,
            Browser::Safari,
        ]
    }

    /// Get browser name for display
    pub fn name(&self) -> &'static str {
        match self {
            Browser::Chrome => "Chrome",
            Browser::Firefox => "Firefox",
            Browser::Edge => "Edge",
            Browser::Safari => "Safari",
            Browser::Opera => "Opera",
            Browser::OperaGX => "Opera GX",
            Browser::Brave => "Brave",
            Browser::Chromium => "Chromium",
            Browser::Vivaldi => "Vivaldi",
        }
    }

    /// Extract cookies from this browser for a specific domain
    pub fn extract_cookies(&self, domain: &str) -> Result<HashMap<String, String>> {
        let cookies = match self {
            Browser::Chrome => rookie::chrome(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            Browser::Firefox => rookie::firefox(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            Browser::Edge => rookie::edge(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            Browser::Safari => {
                #[cfg(target_os = "macos")]
                {
                    rookie::safari(Some(vec![domain.to_string()]))
                        .map_err(|e| anyhow::anyhow!("{}", e))?
                }
                #[cfg(not(target_os = "macos"))]
                {
                    anyhow::bail!("Safari is only available on macOS")
                }
            }
            Browser::Opera => rookie::opera(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            Browser::OperaGX => rookie::opera_gx(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            Browser::Brave => rookie::brave(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            Browser::Chromium => rookie::chromium(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            Browser::Vivaldi => rookie::vivaldi(Some(vec![domain.to_string()]))
                .map_err(|e| anyhow::anyhow!("{}", e))?,
        };

        let mut cookie_map = HashMap::new();
        for cookie in cookies {
            cookie_map.insert(cookie.name, cookie.value);
        }

        Ok(cookie_map)
    }
}

/// Try to extract cookies from all supported browsers
/// Returns the first browser that has cookies for the domain
pub fn extract_cookies_multi_browser(domain: &str) -> Result<HashMap<String, String>> {
    let browsers = Browser::all();

    for browser in browsers {
        match browser.extract_cookies(domain) {
            Ok(cookies) if !cookies.is_empty() => {
                println!("  ✅ Found cookies in {} browser", browser.name());
                return Ok(cookies);
            }
            Ok(_) => {
                // Browser found but no cookies for this domain
                continue;
            }
            Err(_) => {
                // Browser not installed or can't read cookies
                continue;
            }
        }
    }

    // No browser had cookies
    anyhow::bail!(
        "Could not find cookies for {} in any supported browser.\n\
        \n\
        Supported browsers: Chrome, Edge, Brave, Opera, Opera GX, Firefox, Chromium, Vivaldi, Safari\n\
        \n\
        Please:\n\
        1. Log into {} in one of the supported browsers\n\
        2. Make sure you're logged in and can access the site\n\
        3. Try running this program as Administrator (for Chrome v130+)",
        domain,
        domain
    )
}

/// Extract ChatGPT session token from browsers
pub fn extract_chatgpt_session() -> Result<String> {
    let cookies = extract_cookies_multi_browser("chat.openai.com")?;

    // Try multiple possible cookie names
    let token = cookies
        .get("__Secure-next-auth.session-token")
        .or_else(|| cookies.get("access-token"))
        .or_else(|| cookies.get("cf_clearance"))
        .context("No ChatGPT session token found in browser cookies")?;

    Ok(token.clone())
}

/// Extract Google Gemini session cookies from browsers
pub fn extract_gemini_session() -> Result<HashMap<String, String>> {
    let cookies = extract_cookies_multi_browser("gemini.google.com")?;

    // Google uses multiple cookies for authentication
    let required_cookies = vec!["SID", "HSID", "SSID", "__Secure-1PSID", "__Secure-3PSID"];
    let mut session_cookies = HashMap::new();

    for cookie_name in required_cookies {
        if let Some(value) = cookies.get(cookie_name) {
            session_cookies.insert(cookie_name.to_string(), value.clone());
        }
    }

    if session_cookies.is_empty() {
        anyhow::bail!("No Gemini session cookies found in browser");
    }

    Ok(session_cookies)
}

/// Extract Claude session token from browsers
pub fn extract_claude_session() -> Result<String> {
    let cookies = extract_cookies_multi_browser("claude.ai")?;

    let token = cookies
        .get("sessionKey")
        .or_else(|| cookies.get("__cf_bm"))
        .context("No Claude session token found in browser cookies")?;

    Ok(token.clone())
}

/// Extract Poe session token from browsers
#[allow(dead_code)]
pub fn extract_poe_session() -> Result<String> {
    let cookies = extract_cookies_multi_browser("poe.com")?;

    let token = cookies
        .get("p-b")
        .or_else(|| cookies.get("p-lat"))
        .context("No Poe session token found in browser cookies")?;

    Ok(token.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_detection() {
        let browsers = Browser::all();
        println!("Testing {} browsers", browsers.len());

        for browser in browsers {
            match browser.extract_cookies("google.com") {
                Ok(cookies) => {
                    println!("  ✅ {} - Found {} cookies", browser.name(), cookies.len())
                }
                Err(_) => println!("  ⚠️  {} - Not available", browser.name()),
            }
        }
    }

    #[test]
    fn test_multi_browser_extraction() {
        match extract_cookies_multi_browser("google.com") {
            Ok(cookies) => println!("Found {} cookies for google.com", cookies.len()),
            Err(e) => println!("Could not extract cookies: {}", e),
        }
    }
}
