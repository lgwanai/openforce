-- SQL Query to Calculate Annual Service Fees for Specific Suppliers
-- Aggregation: Annual Total (No Enterprise Grouping)
-- Time Range: OUT_DATE >= '2025-01-01' AND OUT_DATE < '2026-01-01'
-- Status: '已离店' (Checked Out)
-- Suppliers: '天下房仓1', '差旅壹号1'

WITH OrderData AS (
    SELECT 
        *,
        -- 24、25 年直接使用 ROOM_NIGHTS，23 年使用 TOTAL_PRICE / NULLIF(ROOM_PRICE, 0)
        CASE 
            WHEN OUT_DATE >= '2024-01-01' THEN ROOM_NIGHTS
            ELSE TOTAL_PRICE / NULLIF(ROOM_PRICE, 0)
        END AS real_room_nights,
        -- 单价直接使用 ROOM_PRICE
        ROOM_PRICE AS actual_nightly_rate,
        -- 提取日期部分
        toDate(OUT_DATE) AS day_date
    FROM 
        hotel.dwd_hotel_order 
    WHERE 
        OUT_DATE >= '2025-01-01' 
        AND OUT_DATE < '2026-01-01' 
        AND STATUS = '已离店' 
        AND (ROOM_PRICE > 0 OR ROOM_NIGHTS > 0)
        AND SUPPLIER_NO IN ('天下房仓1', '差旅壹号1')
        AND (EP_TYPE != '测试' OR EP_TYPE IS NULL)
)
SELECT 
    day_date AS "日期",
    SUPPLIER_NO AS "供应商编号",
    PAYMENT_TYPE AS "支付方式",
    SUM(real_room_nights) AS "总间夜量",
    SUM(TOTAL_PRICE) AS "总GMV",
    
    -- 符合条件的间夜全量平均房费：总金额 / 总间夜数
    SUM(TOTAL_PRICE) / NULLIF(SUM(real_room_nights), 0) AS "平均房费",
    
    -- Service Fee Rule 1 (General): Min(TOTAL_PRICE * 0.07, 35) per order
    SUM(
        CASE 
            WHEN TOTAL_PRICE * 0.07 < 35 THEN TOTAL_PRICE * 0.07 
            ELSE 35 
        END
    ) AS "通用服务费(7%或35元)",
    
    -- Service Fee Rule 2 (Supplier Specific):
    --   Prepaid: 10 * 80% * (1 + 6%) per room night
    --   Pay at Hotel: 10 * 30% * (1 + 6%) per room night
    SUM(
        CASE 
            WHEN PAYMENT_TYPE = '公司统一支付' THEN real_room_nights * 10 * 0.8 * 1.06 
            WHEN PAYMENT_TYPE = '到店付' THEN real_room_nights * 10 * 0.3 * 1.06 
            ELSE 0 
        END
    ) AS "差旅壹号服务费",
    
    -- Service Fee Rule 3 (New): Min(actual_nightly_rate * 3%, 8) per room night
    SUM(
        real_room_nights * (
            CASE 
                WHEN actual_nightly_rate * 0.03 < 8 THEN actual_nightly_rate * 0.03 
                ELSE 8 
            END
        )
    ) AS "新服务费(3%或8元)",
    
    -- Service Fee Rule 4 (Simple 1.7%): 1.7% of Total Price
    SUM(TOTAL_PRICE * 0.017) AS "简单新服务费(1.7%)",
    
    -- Per-night fee stats based on the simple 1.7% rule (actual_nightly_rate * 1.7%)
    MIN(actual_nightly_rate * 0.017) AS "每间夜最低服务费(按1.7%算)",
    
    MAX(actual_nightly_rate * 0.017) AS "每间夜最高服务费(按1.7%算)"
    
FROM 
    OrderData
GROUP BY
    day_date,
    SUPPLIER_NO,
    PAYMENT_TYPE
ORDER BY
    day_date,
    SUPPLIER_NO,
    PAYMENT_TYPE;