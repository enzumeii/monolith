# Convention administration and logo storage

## Saved and draft state

| Concern | Behavior |
| --- | --- |
| Identity and currency | Slug and one convention currency are immutable after creation; final prices are exact decimal strings backed by PostgreSQL NUMERIC |
| Preorders | New conventions are disabled; administrators explicitly enable or disable the single preorder flag. Event calendar dates do not schedule sales |
| Currency/payment choices | Authenticated options endpoint derives supported currencies and payment methods from the existing currency precision and provider rules; no runtime currency allowlist is required |
| Metadata, selection and logo | Separate save operations and persisted snapshots; failed requests retain drafts. Creation may upload a logo before inserting metadata; retries retain its uploaded location |
| Membership replacement | Load every saved membership page before editing or saving; catalogue search/pagination never replace the selected map. Backend replaces up to250 memberships atomically |
| Source information | Catalogue displays product title, variant title, SKU, actual variant image, source currency/price and available quantity at the submitted location. Missing images and SKUs have explicit placeholders; source stock is informational |
| Selected variant display | Readonly details are separate from editable membership and exact overridden prices. Search/page changes and legacy saves retain observed details in the current editor; explicit null clears stale details. Enriched saved responses populate fresh views; legacy reloads show unavailable fields truthfully |
| Quantity provenance | Catalogue observations retain their submitted location. Saved-response quantities have no invented location. Missing saved quantities show “Not available”; zero and negative quantities remain numeric. No live refresh or per-row provider requests |
| Backend dependency | The UI accepts optional saved display fields ahead of the deferred PR #57 rebase. The pinned merged backend does not yet persist or return those fields or selected quantities; current-editor observations are not persisted by this change |
| Navigation | Shared session-scoped draft guard preserves the active editor, confirms departure, and restores native history before proceeding; unsupported browser history protection is reported |
| Markdown | Shared CommonMark preview escapes raw HTML and sanitizes a strict element/attribute/link allowlist; no executable embedded HTML, CSS, images or unsafe schemes |
| Shared presentation | Both applications use the same common stylesheet and form/error/loading primitives; feature requests and drafts remain backoffice-owned |

## Logo upload and read

```mermaid
sequenceDiagram
    participant Admin as Admin browser
    participant API as Authenticated API
    participant Storage as Private S3
    participant DB as PostgreSQL
    Admin->>API: POST files?location=conventions/slug (PNG/JPEG)
    API->>API: Validate folder, bound and decode image
    API->>Storage: PUT conventions/slug/UUID.ext
    Storage-->>API: Accepted
    API-->>Admin: Stable location, signed URL and expiry
    Admin->>API: Create or PATCH with logo_location
    API->>Storage: HEAD then bounded GET and decode
    API->>DB: Persist location with convention metadata
    DB-->>API: Saved convention
    API-->>Admin: Location, short-lived URL and expiry
    Admin->>Storage: GET signed image URL
    Storage-->>Admin: Image bytes
    Admin->>API: GET convention to refresh preview
    API-->>Admin: Fresh signed URL and expiry
```

| Contract | Behavior |
| --- | --- |
| Upload | Authenticated ADMIN/SUPER_ADMIN can upload before convention creation using `POST /api/v1/admin/files?location=conventions/<slug>`. PNG/JPEG must match Content-Type; maximum2MiB, dimensions1–4096 per axis, bounded decode allocation64MiB |
| Object location | Frontend selects the convention slug folder; backend generates `conventions/<slug>/<UUID>.png` or `.jpg`. No client-selected filename, bucket, provider URL or report namespace |
| Persistence | Create accepts optional `logo_location`; PATCH preserves missing, clears null, or attaches a validated location for the immutable slug. Database stores the object key, never an expiring URL |
| Attachment | HEAD metadata plus bounded GET and complete image decode validate the object before writing its reference. Missing/invalid objects or provider failures cannot report attachment success |
| Rendering | Convention responses return `logo_location`, transient `logo_url` and RFC3339 `logo_url_expires_at`. Signing uses existing S3 TTL (checked-in300s, maximum900s); signed image GET needs no browser API credentials |
| Preview availability | Metadata reads/saves remain usable when preview signing is unavailable: location remains, URL/expiry are null and the UI offers refresh. Image failure or expiry has an explicit refresh action that updates only preview state; no editor reload or automatic retry loop |
| Drafts and retries | Discard/success recreate only the native file input, allowing same-file reselection. Upload/attach failure preserves candidate and metadata/price drafts; an uploaded location is reused for attachment retry. Creation slug changes invalidate its staged location |
| Replace/clear/delete | Old signed URLs can remain usable until expiry. Old/unreferenced objects remain private in storage; no immediate link revocation or garbage collection is provided |
| Failure ordering | Upload signs before PUT. Attachment validates before DB writes. A saved metadata response does not report failure solely because its optional preview could not be signed |
| Storage isolation | Existing private report PUT and signed CSV attachment contracts retain their behavior. Image signing shares transport/destination validation while using inline image semantics |


Local verification uses controlled S3 and Shopify servers and Docker PostgreSQL; results and provider/browser limits belong in [verification](verification.md).
