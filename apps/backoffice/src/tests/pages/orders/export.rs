use super::*;
use domain::dto::orders::{FastshipLabel, ThaiPostLabel};

fn fastship_label() -> FastshipLabel {
    FastshipLabel {
        firstname: "Alice".into(),
        lastname: "Smith".into(),
        email: "alice@example.test".into(),
        phone: "123".into(),
        address_1: "Street 1".into(),
        address_2: String::new(),
        city: "Bangkok".into(),
        state: String::new(),
        postcode: "10000".into(),
        country: "TH".into(),
        width: 1,
        height: 2,
        length: 3,
        weight: 4,
        category: "Goods".into(),
        quantity: 5,
        declared_value: 6_u64.into(),
        ioss: String::new(),
        note: String::new(),
        shipping_option: "Express".into(),
        reference_id: "order-7".into(),
        incoterm: "DDP".into(),
    }
}

fn thai_post_label() -> ThaiPostLabel {
    ThaiPostLabel {
        country_code: "US".into(),
        weight: 321,
        service_id: 2,
        category: "Goods".into(),
        sender_name: "Sender".into(),
        sender_phone: "123".into(),
        sender_email: "sender@example.test".into(),
        sender_address: "Sender address".into(),
        sender_sub_district: String::new(),
        sender_district: String::new(),
        sender_province: "Bangkok".into(),
        sender_postcode: "10000".into(),
        sender_country_code: "TH".into(),
        receiver_name: "Receiver".into(),
        receiver_phone: "456".into(),
        receiver_email: "receiver@example.test".into(),
        receiver_address: "Receiver address".into(),
        receiver_state: "CA".into(),
        receiver_city: "Los Angeles".into(),
        receiver_postcode: "90001".into(),
        currency: "USD".into(),
        item_description: "Sample".into(),
        quantity: 7,
        price: 20_u64.into(),
        origin: "TH".into(),
        ioss: String::new(),
        order_id: "1234".into(),
        insurance: "false".into(),
        hs_code: "0000".into(),
    }
}

fn xml(bytes: &[u8], path: &str) -> String {
    use std::io::{Cursor, Read};
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("XLSX archive");
    let mut text = String::new();
    archive
        .by_name(path)
        .expect("XLSX entry")
        .read_to_string(&mut text)
        .expect("XML");
    text
}

fn workbook_row(bytes: &[u8], row: usize, columns: usize) -> Vec<String> {
    let strings = xml(bytes, "xl/sharedStrings.xml");
    let strings = strings
        .split("<si>")
        .skip(1)
        .map(|item| {
            let text = item.split_once("<t").expect("text start").1;
            text.split_once('>')
                .expect("text attribute end")
                .1
                .split_once("</t>")
                .expect("text end")
                .0
                .to_owned()
        })
        .collect::<Vec<_>>();
    let sheet = xml(bytes, "xl/worksheets/sheet1.xml");
    let row = sheet
        .split_once(&format!("<row r=\"{row}\""))
        .expect("sheet row")
        .1
        .split_once("</row>")
        .expect("row end")
        .0;
    let mut cells = vec![String::new(); columns];
    for cell in row.split("<c r=\"").skip(1) {
        let reference = cell.split_once('"').expect("cell reference").0;
        let column = reference
            .bytes()
            .take_while(u8::is_ascii_uppercase)
            .fold(0_usize, |index, byte| {
                index * 26 + usize::from(byte - b'A' + 1)
            });
        let index = cell
            .split_once("<v>")
            .expect("shared value")
            .1
            .split_once("</v>")
            .expect("value end")
            .0
            .parse::<usize>()
            .expect("shared string index");
        cells[column - 1] = strings[index].clone();
    }
    cells
}

#[test]
fn fastship_csv_matches_legacy_headers_and_aligned_values() {
    let table = fastship_table(&[fastship_label()]);
    let headers = [
        "Firstname",
        "Lastname",
        "Email",
        "Phone",
        "Address Line 1",
        "Address Line 2",
        "City",
        "State",
        "Postcode",
        "Country",
        "Width",
        "Height",
        "Length",
        "Weight",
        "Category",
        "Qty",
        "Declare Value",
        "IOSS",
        "Note",
        "Shipping Option",
        "ReferenceId",
        "Incoterm",
    ];
    let values = [
        "Alice",
        "Smith",
        "alice@example.test",
        "123",
        "Street 1",
        "",
        "Bangkok",
        "",
        "10000",
        "TH",
        "1",
        "2",
        "3",
        "4",
        "Goods",
        "5",
        "6",
        "",
        "",
        "Express",
        "order-7",
        "DDP",
    ];
    let csv = crate::export::encode_table(table.headers, &table.rows);
    let mut lines = csv.split("\r\n");
    assert_eq!(
        lines
            .next()
            .expect("CSV header")
            .split(',')
            .collect::<Vec<_>>(),
        headers
    );
    assert_eq!(
        lines
            .next()
            .expect("CSV values")
            .split(',')
            .collect::<Vec<_>>(),
        values
    );
}

#[test]
fn thai_post_xlsx_matches_legacy_headers_and_aligned_values() {
    let table = thai_post_table(&[thai_post_label()]);
    let headers = [
        "รหัสประเทศ (Country Code)",
        "น้ำหนักรวม (Total Weight (g.))",
        "รหัสบริการ (Service ID)",
        "ประเภทสิ่งของ (Category)",
        "ชื่อ-นามสกุลผู้ฝากส่ง (Sender Name)",
        "เบอร์โทรศัพท์ผู้ฝากส่ง (Tel.)",
        "อีเมล (E-mail) ผู้ฝากส่ง",
        "ที่อยู่ผู้ฝากส่ง (Sender Address)",
        "แขวง (Sub-District)",
        "เขต (District)",
        "จังหวัด (Province)",
        "รหัสประเทศ (Postcode) ",
        "รหัสประเทศ (Country Code) ",
        "ชื่อ-นามสกุลผู้รับ (Receiver Name)",
        "เบอร์โทรศัพท์ผู้รับ (Tel.)",
        "อีเมล (E-mail) ผู้รับ",
        "ที่อยู่ผู้รับ (Receiver Address)",
        "เขต (State/Province)",
        "เมือง (City/District)",
        "รหัสประเทศ (Postcode)",
        "ค่าเงิน (Currency)",
        "สิ่งของภายใน (Item Description)",
        "จำนวน (Quantity)",
        "น้ำหนัก (Net Weight (g.))",
        "มูลค่า (Price)",
        "ประเทศกำเนิดสิ่งของ (Origin)",
        "IOSS Number",
        "ORDER_ID",
        "INSURANCE",
        "พิกัดศุลกากร (HS Code)",
    ];
    let values = [
        "US",
        "321",
        "2",
        "Goods",
        "Sender",
        "123",
        "sender@example.test",
        "Sender address",
        "",
        "",
        "Bangkok",
        "10000",
        "TH",
        "Receiver",
        "456",
        "receiver@example.test",
        "Receiver address",
        "CA",
        "Los Angeles",
        "90001",
        "USD",
        "Sample",
        "7",
        "321",
        "20",
        "TH",
        "",
        "1234",
        "false",
        "0000",
    ];
    let bytes =
        crate::export::encode_table_xlsx(table.headers, &table.rows, "ThaiPost").expect("XLSX");
    assert_eq!(workbook_row(&bytes, 1, 30), headers);
    assert_eq!(workbook_row(&bytes, 2, 30), values);
    assert!(!xml(&bytes, "xl/worksheets/sheet1.xml").contains("<f>"));
}

#[test]
fn fulfillment_export_names_all_four_couriers() {
    use domain::dto::orders::Courier;
    for (courier, name) in [
        (Courier::Fastship, "FASTSHIP"),
        (Courier::ThaiPost, "THAI_POST"),
        (Courier::Best, "BEST"),
        (Courier::Teleport, "TELEPORT"),
    ] {
        let row = FulfillmentOrder {
            order: domain::dto::orders::OrderSummary {
                id: 42,
                number: 1042,
                full_name: None,
                email: None,
                shipping_country_code: None,
            },
            courier,
        };
        let table = fulfillment_table(&[row], &std::collections::HashMap::new(), &[]);
        assert_eq!(table.rows[0][4], name);
    }
}

use crate::pages::orders::tests::{best_label, teleport_label};

#[test]
fn best_xlsx_preserves_legacy_headers_and_values() {
    let table = best_table(&[best_label()]);
    let headers = [
        "*Recipient's name",
        "*Recipient's telephone#",
        "*Recipient's post code",
        "*Recipient's address detail",
        "*Commodity",
        "COD amount",
        "Insured amount",
        "Waybill#",
        "Note",
        "parcel ID",
        "Weight",
    ];
    let values = [
        "ผู้รับ Best",
        "00123",
        "00100",
        "Best address",
        "Goods",
        "12.50",
        "15",
        "WB001",
        "=1+1",
        "00042",
        "321",
    ];
    let bytes =
        crate::export::encode_table_xlsx(table.headers, &table.rows, "Best").expect("workbook");
    assert_eq!(workbook_row(&bytes, 1, 11), headers);
    assert_eq!(workbook_row(&bytes, 2, 11), values);
    assert!(!xml(&bytes, "xl/worksheets/sheet1.xml").contains("<f>"));
}

#[test]
fn teleport_xlsx_preserves_legacy_headers_and_values() {
    let table = teleport_table(&[teleport_label()]);
    let headers = [
        "Recipient Name*",
        "Recipient Email*",
        "Recipient Phone No*",
        "Recipient Address*",
        "Recipient State*",
        "Recipient City*",
        "Recipient Postal Code*",
        "Package*",
        "Length (cm)*",
        "Width (cm)*",
        "Height (cm)*",
        "Weight (kg)*",
        "Multi Piece/ Single Piece*",
        "Quantity ( pieces)*",
        "Commodity*",
        "Commodity Description*",
        "Commodity Currency*",
        "Commodity Value*",
        "Delivery Instruction (optional)",
        "Tracking Number (optional)",
        "Reference  (optional)",
        "Destination geo  (optional)",
    ];
    let values = [
        "ผู้รับ Teleport",
        "buyer@example.test",
        "00456",
        "Teleport address",
        "Bangkok",
        "City",
        "00101",
        "Polymailer",
        "13",
        "15",
        "4",
        "0.321",
        "Multi Piece Shippment",
        "3",
        "Goods",
        "=SUM(1)",
        "THB",
        "12.50",
        "Call first",
        "000123",
        "ref-42",
        "13.75,100.5",
    ];
    let bytes =
        crate::export::encode_table_xlsx(table.headers, &table.rows, "Teleport").expect("workbook");
    assert_eq!(workbook_row(&bytes, 1, 22), headers);
    assert_eq!(workbook_row(&bytes, 2, 22), values);
    assert!(!xml(&bytes, "xl/worksheets/sheet1.xml").contains("<f>"));
}
