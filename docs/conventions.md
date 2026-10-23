# Convention administration and logo storage

## Saved and draft state

| Concern | Behavior |
| --- | --- |
| Identity and currency | Slug and one convention currency are immutable after creation; final prices are exact decimal strings backed by PostgreSQL NUMERIC |
| Preorders | New conventions are disabled; administrators explicitly enable or disable the single preorder flag. Event calendar dates do not schedule sales |
| Country/currency choices | Authenticated options endpoint returns sorted assigned country codes plus supported currencies/payment methods from existing validation and precision/provider rules. Forms use selects; saved currency remains immutable |
| Metadata, selection and logo | Separate save operations and persisted snapshots; failed requests retain drafts. Creation may upload a logo before inserting metadata; retries retain its uploaded location |
| Membership replacement | Load every saved membership page before editing or saving; catalogue search/pagination never replace the selected map. Backend replaces up to250 memberships atomically |
| Catalogue picker | Browse Shopify catalogue opens the shared popup with Product type and Product title text fields. Nonblank escaped clauses intersect with AND; empty filters browse eligible products. Every page uses the numeric location configured as `65761018027` in public backoffice settings. Closing retains results, selected variants and price drafts |
| Source information | Compact result rows display separate small product/actual variant images, titles, SKU, source currency/price and available quantity. Missing images and SKUs have explicit placeholders; source stock is informational |
| Selected variant display | Readonly details are separate from editable membership and exact overridden prices. Search/page changes and legacy saves retain observed details in the current editor; explicit null clears stale details. Enriched saved responses populate fresh views; legacy reloads show unavailable fields truthfully |
| Quantity provenance | Catalogue observations retain their submitted location. Saved-response quantities have no invented location. Missing saved quantities show “Not available”; zero and negative quantities remain numeric. No live refresh or per-row provider requests |
| Saved display snapshots | Required Shopify validation persists product/variant titles, nullable SKU, product/variant image URLs and inventory policy with the price. GET reads those snapshots without a provider call; older rows remain nullable until required validation. The UI uses the actual variant image and keeps explicit null distinct from omitted legacy fields |
| Saved quantity limit | Selected responses do not include available quantity. Current-editor catalogue observations retain their location, while fresh saved views show “Not available”; no quantity persistence or extra stock lookup is introduced |
| Navigation | Shared session-scoped draft guard preserves the active editor and confirms departure. Successful creation clears completion/dirty state and returns to the list; failures preserve drafts and staged upload for retry |
| Markdown | Shared CommonMark preview escapes raw HTML and sanitizes a strict element/attribute/link allowlist; no executable embedded HTML, CSS, images or unsafe schemes |
| Shared presentation | Shared button styles cover native controls and navigation actions. Convention rows show safe compact logo previews; labels are smaller/muted and values stronger. Enabled badges/actions use green cues; disabled states red with text/native semantics. Feature state and requests remain backoffice-owned |

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
    Admin->>API: Create or PATCH with logo_url
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
| Object location | Request location includes bucket and folder: `conventions/<slug>` selects bucket `conventions` and folder `<slug>`. Backend generates the UUID filename and returns `conventions/<slug>/<UUID>.png` or `.jpg`; upload, attachment and preview all use that same bucket/key split. No report-bucket prefix or client-selected filename/provider URL |
| Persistence | Create accepts optional `logo_url`; PATCH preserves missing, clears null, or attaches a validated location for the immutable slug. The existing database `logo_url` column stores the full bucket/key location, never an expiring URL |
| Published schema | The original CREATE migration remains unchanged. The existing `logo_url` column stores uploaded S3 object locations. Legacy HTTPS values remain stored without S3 preview; an administrator can upload and attach an image through the normal flow |
| Attachment | HEAD metadata plus bounded GET and complete image decode validate the object before writing its reference. Missing/invalid objects or provider failures cannot report attachment success |
| Rendering | Convention responses omit the stored `logo_url` and return only transient `logo_preview_url` and RFC3339 `logo_preview_url_expires_at`. Signing uses existing S3 TTL (checked-in300s, maximum900s); signed image GET needs no browser API credentials |
| Preview availability | Create/list/get/PATCH require the shared S3 client and return503 when unavailable. A signing failure with a configured client preserves metadata, returns null URL/expiry and leaves the stored location unchanged; the UI retries preview loading automatically. Preview URLs refresh automatically 30 seconds before expiry and after image failure, with retries limited to one per minute. Refresh updates only preview state and preserves editor drafts |
| Drafts and retries | Discard/success recreate only the native file input, allowing same-file reselection. Upload/attach failure preserves candidate and metadata/price drafts; an uploaded location is reused for attachment retry. Creation slug changes invalidate its staged location |
| Replace/clear/delete | Old signed URLs can remain usable until expiry. Old/unreferenced objects remain private in storage; no immediate link revocation or garbage collection is provided |
| Failure ordering | Upload signs before PUT. Attachment validates before DB writes. A saved metadata response does not report failure solely because its optional preview could not be signed |
| Storage isolation | Existing private report PUT and signed CSV attachment contracts retain their behavior. Image signing shares transport/destination validation while using inline image semantics |


Local verification uses controlled S3 and Shopify servers and Docker PostgreSQL; results and provider/browser limits belong in [verification](verification.md).
