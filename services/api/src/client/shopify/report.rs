use super::{ORIGIN, ShopifyClient, require_lines};
use crate::{
    client::{
        ClientError,
        pagination::{collect_connection, collect_pages},
    },
    models::shopify::{
        ListProductsRequest, OrderLinesData, OrderLinesRequest,
        report::{InventoryReportData, ReportLine, ReportProduct, VariantPageData},
    },
};
use serde_json::json;

impl ShopifyClient {
    pub async fn inventory_report_products(
        &self,
        input: &ListProductsRequest,
    ) -> Result<Option<Vec<ReportProduct>>, ClientError> {
        let location: crate::models::shopify::report::LocationData = self
            .execute(
                "ReportLocation",
                include_str!("report/location.graphql"),
                json!({"id":input.location_id}),
            )
            .await
            .map_err(|error| error.at(ORIGIN))?;
        if location.location.is_none() {
            return Ok(None);
        }
        let first: InventoryReportData = self
            .execute(
                "ReportProductsWithVariants",
                include_str!("report/products_with_variants.graphql"),
                input.variables(None)?,
            )
            .await
            .map_err(|error| error.at(ORIGIN))?;
        if first.location.is_none() {
            return Ok(None);
        }
        let mut initial = Some(first.products);
        let products = collect_pages(|after| {
            let first = initial.take();
            async move {
                if let Some(first) = first {
                    return Ok(first);
                }
                let data: InventoryReportData = self
                    .execute(
                        "ReportProductsWithVariants",
                        include_str!("report/products_with_variants.graphql"),
                        input.variables(after.as_deref())?,
                    )
                    .await?;
                if data.location.is_none() {
                    return Err(ClientError::Protocol("report location disappeared"));
                }
                Ok(data.products)
            }
        })
        .await
        .map_err(|error| error.at(ORIGIN))?;
        let mut result = Vec::new();
        for mut product in products {
            let id = product.id.clone();
            let mut initial = Some(product.variants);
            product.variants = collect_connection(|after| {
                let first = initial.take();
                let id = &id;
                async move {
                    if let Some(first) = first {
                        return Ok(first);
                    }
                    let data: VariantPageData = self
                        .execute(
                            "ReportProductVariants",
                            include_str!("report/product_variants.graphql"),
                            json!({"id":id,"locationId":input.location_id,"after":after}),
                        )
                        .await?;
                    let parent = data
                        .product
                        .ok_or(ClientError::Protocol("report product disappeared"))?;
                    if &parent.id != id {
                        return Err(ClientError::Protocol(
                            "report variants belong to another product",
                        ));
                    }
                    Ok(parent.variants)
                }
            })
            .await
            .map_err(|error| error.at(ORIGIN))?;
            result.push(product);
        }
        Ok(Some(result))
    }

    pub async fn report_tagged_order_lines(
        &self,
        input: &OrderLinesRequest,
    ) -> Result<Vec<ReportLine>, ClientError> {
        collect_pages(async |after| {
            let data: OrderLinesData<ReportLine> = self
                .execute(
                    "ReportTaggedOrderLines",
                    include_str!("report/tagged_order_lines.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            require_lines(data, &input.order_id)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
}
