//! Tracker-pixel classifier.
//!
//! Email marketing platforms embed 1×1 "open beacons" that ping a
//! tracker host the moment the recipient's mail client loads the image.
//! Even with our image proxy in front (which shields the *IP*), the
//! fact that the beacon loaded still tells the sender the mail has
//! been opened — that's the tracking signal we actually want to kill.
//!
//! This module classifies a URL as belonging to a known tracker
//! service. The sanitizer uses the result to force a blocked
//! placeholder regardless of the per-message "show remote content"
//! toggle. Privacy is the floor, not a preference.
//!
//! Matching is intentionally conservative — we'd rather miss a tracker
//! than break a legit image. The list is seeded from the publicly
//! available EasyPrivacy / uBO email-tracker rules; it's biased toward
//! the services most commonly used by transactional and marketing
//! senders. A future follow-up can ship pluggable rule updates.

/// Classification result — names the service so the UI can be
/// transparent about *why* a pixel was blocked ("blocked 3 trackers
/// from Mailchimp and SendGrid") rather than vaguely waving at
/// "privacy".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerMatch {
    pub service: &'static str,
    pub host: String,
}

/// Host suffix → service name. "Suffix" matches if the request host
/// either equals the entry exactly, or ends with `.` + entry. So
/// `beacon.list-manage.com` matches `list-manage.com`, but `notmy-
/// list-manage.com` does not.
///
/// Entries are lowercased; the classifier lowercases the input host
/// before comparing.
const TRACKER_HOSTS: &[(&str, &str)] = &[
    // Mailchimp family
    ("list-manage.com", "Mailchimp"),
    ("mailchi.mp", "Mailchimp"),
    // SendGrid / Twilio
    ("sendgrid.net", "SendGrid"),
    ("sendgrid.com", "SendGrid"),
    ("sparkpostmail.com", "SparkPost"),
    ("sparkpostmail1.com", "SparkPost"),
    ("mandrillapp.com", "Mandrill"),
    ("email.mandrillapp.com", "Mandrill"),
    // HubSpot
    ("hubspotemail.net", "HubSpot"),
    ("hubspot.com", "HubSpot"),
    ("hs-sites.com", "HubSpot"),
    // Salesforce / Marketing Cloud
    ("exct.net", "Salesforce Marketing Cloud"),
    ("sendfwd.com", "Salesforce Marketing Cloud"),
    ("sfmc-content.com", "Salesforce Marketing Cloud"),
    // Marketo
    ("mktoresp.com", "Marketo"),
    ("mktoweb.com", "Marketo"),
    // Pardot
    ("pardot.com", "Pardot"),
    ("pi.pardot.com", "Pardot"),
    // Constant Contact
    ("constantcontact.com", "Constant Contact"),
    ("roving.com", "Constant Contact"),
    // Klaviyo, Sendinblue, Braze, Iterable
    ("klaviyomail.com", "Klaviyo"),
    ("klclick.com", "Klaviyo"),
    ("klclick1.com", "Klaviyo"),
    ("sendinblue.com", "Sendinblue"),
    ("sibsvc.com", "Sendinblue"),
    ("sib-utils.com", "Sendinblue"),
    ("br-mail01.com", "Braze"),
    ("braze.com", "Braze"),
    ("iterable.com", "Iterable"),
    // Substack (has tracker pixels in open-sent newsletters)
    ("substack.com", "Substack"),
    // Customer.io, Drip, Autopilot
    ("customeriomail.com", "Customer.io"),
    ("trk.cp20.com", "Customer.io"),
    ("drip.com", "Drip"),
    ("s3.amazonaws.com/trk.autopilotapp.com", "Autopilot"),
    // Eloqua / Oracle
    ("eloqua.com", "Eloqua"),
    ("en25.com", "Eloqua"),
    // ICPTrack / Listrak
    ("icptrack.com", "Listrak"),
    ("listrakbi.com", "Listrak"),
    // Responsys
    ("rsys2.com", "Oracle Responsys"),
    ("p.rfer.us", "Oracle Responsys"),
    // Sailthru
    ("sail-through.com", "Sailthru"),
    ("sailthru.com", "Sailthru"),
    // Silverpop / IBM Watson
    ("pages05.net", "Silverpop"),
    ("pages03.net", "Silverpop"),
    ("pages04.net", "Silverpop"),
    ("silverpop.com", "Silverpop"),
    // LinkedIn / Meta / Twitter open-click trackers (often embedded in
    // notification emails)
    ("click.linkedin.com", "LinkedIn"),
    ("em.secureserver.net", "LinkedIn"),
    ("facebookmail.com", "Meta"),
    ("e.twitter.com", "X (Twitter)"),
    // Emarsys, Cordial, Postmark
    ("emarsys.com", "Emarsys"),
    ("emaildmctr.com", "Cordial"),
    ("pstmrk.it", "Postmark"),
    // Litmus (used for test-send open tracking)
    ("litmusemail.com", "Litmus"),
    // Campaign Monitor
    ("createsend.com", "Campaign Monitor"),
    ("cmail1.com", "Campaign Monitor"),
    ("cmail2.com", "Campaign Monitor"),
    ("cmail19.com", "Campaign Monitor"),
    ("cmail20.com", "Campaign Monitor"),
    // MailerLite, GetResponse
    ("mlsend.com", "MailerLite"),
    ("mailerlite.com", "MailerLite"),
    ("getresponse.com", "GetResponse"),
    ("gr-host.com", "GetResponse"),
    // ActiveCampaign
    ("activehosted.com", "ActiveCampaign"),
    ("activecampaign.com", "ActiveCampaign"),
    // Generic analytics beacons sometimes used in HTML mail
    ("google-analytics.com", "Google Analytics"),
    ("googletagmanager.com", "Google Tag Manager"),
    ("doubleclick.net", "DoubleClick"),
    ("facebook.com/tr", "Meta Pixel"),
    // Amazon SES open tracking wrapper
    ("r.email-od.com", "Amazon SES"),
    ("awstrack.me", "Amazon SES"),
    ("sesmailer.com", "Amazon SES"),
];

/// URL path substrings that strongly imply a tracker beacon. Used as
/// fallback when the host is legit (e.g. a company's own domain) but
/// the URL path screams "open pixel". Matched case-insensitively
/// against `path + '?' + query`.
const TRACKER_PATH_PATTERNS: &[&str] = &[
    "/open.aspx",
    "/open.gif",
    "/open.png",
    "/o.gif",
    "/pixel.gif",
    "/pixel.png",
    "/track/open",
    "/track/opens",
    "/tracking/open",
    "/email/open",
    "/e/o/",
    "/e/open",
    "/wf/open",
    "/beacon.gif",
    "/beacon.png",
    "/utm.gif",
    "/1x1.gif",
    "/spacer.gif",
    "?action=track_open",
    "?open=1",
    "?utm_medium=email&utm_source=open",
    "/ss/c/", // Litmus / iterable signature
    "/ls/click?", // generic list-serve click redirect
];

/// Lowercase + strip leading "www." so host comparisons are stable.
fn normalize_host(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    lower
        .strip_prefix("www.")
        .map(str::to_owned)
        .unwrap_or(lower)
}

fn extract_host_and_pathquery(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return None;
    }
    let after_scheme = trimmed.splitn(2, "//").nth(1)?;
    let (authority, tail) = match after_scheme.find(['/', '?', '#']) {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => (after_scheme, ""),
    };
    let host = authority.split('@').last()?.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    // Strip the fragment — the server never sees it anyway, and it's
    // not useful for classification.
    let path_query = match tail.find('#') {
        Some(idx) => &tail[..idx],
        None => tail,
    };
    Some((host.to_owned(), path_query.to_owned()))
}

/// Classify `url` as a known tracker beacon if we recognise its host
/// or its URL path shape. Returns `None` for URLs we consider benign.
pub fn classify(url: &str) -> Option<TrackerMatch> {
    let (raw_host, path_query) = extract_host_and_pathquery(url)?;
    let host = normalize_host(&raw_host);
    // Host-suffix match first — cheapest and most specific.
    for (suffix, service) in TRACKER_HOSTS {
        let suffix_lower = suffix.to_ascii_lowercase();
        if host == suffix_lower || host.ends_with(&format!(".{suffix_lower}")) {
            return Some(TrackerMatch {
                service,
                host: raw_host,
            });
        }
    }
    // Path-pattern fallback. Some senders host their beacons on their
    // own domain (custom tracking domain) so host matching misses.
    let lower_pq = path_query.to_ascii_lowercase();
    for pat in TRACKER_PATH_PATTERNS {
        if lower_pq.contains(pat) {
            return Some(TrackerMatch {
                service: "Tracking pixel",
                host: raw_host,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_host() {
        let m = classify("https://list-manage.com/track/open.php?u=1").unwrap();
        assert_eq!(m.service, "Mailchimp");
    }

    #[test]
    fn matches_subdomain() {
        let m = classify("https://beacon.list-manage.com/pixel.gif").unwrap();
        assert_eq!(m.service, "Mailchimp");
        assert_eq!(m.host, "beacon.list-manage.com");
    }

    #[test]
    fn case_insensitive_host() {
        let m = classify("https://BEACON.List-Manage.com/pixel.gif").unwrap();
        assert_eq!(m.service, "Mailchimp");
    }

    #[test]
    fn ignores_benign_host() {
        assert!(classify("https://example.com/image.png").is_none());
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(classify("cid:some-image").is_none());
        assert!(classify("data:image/gif;base64,abc").is_none());
    }

    #[test]
    fn path_pattern_fallback() {
        // Host is not in our list but the path looks like a beacon
        let m = classify("https://news.example.com/track/open?m=abc").unwrap();
        assert_eq!(m.service, "Tracking pixel");
    }

    #[test]
    fn www_stripped_for_match() {
        let m = classify("https://www.sendgrid.net/pixel").unwrap();
        assert_eq!(m.service, "SendGrid");
    }

    #[test]
    fn near_miss_does_not_match() {
        // "notmy-list-manage.com" must NOT match "list-manage.com"
        assert!(classify("https://notmy-list-manage.com/x.gif").is_none());
    }

    #[test]
    fn fragment_stripped() {
        let m = classify("https://BEACON.list-manage.com/pixel.gif#anchor");
        assert!(m.is_some());
    }
}
