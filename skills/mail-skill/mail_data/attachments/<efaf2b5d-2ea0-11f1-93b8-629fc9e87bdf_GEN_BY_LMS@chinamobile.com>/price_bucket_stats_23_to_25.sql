-- SQL Query to get Room Nights distribution by 10 RMB Price Buckets for 2023, 2024, 2025
-- Filtering based on specified conditions

WITH OrderData AS (
    SELECT 
        -- 提取年份
        toYear(OUT_DATE) AS order_year,
        
        -- 订单类型（私人旅行、公务出差）
        ORDER_TYPE,
        
        -- 计算真实间夜：24、25年用ROOM_NIGHTS，23年用TOTAL_PRICE / NULLIF(ROOM_PRICE, 0)
        CASE 
            WHEN OUT_DATE >= '2024-01-01' THEN ROOM_NIGHTS
            ELSE TOTAL_PRICE / NULLIF(ROOM_PRICE, 0)
        END AS real_room_nights,
        
        -- 间夜单价使用 ROOM_PRICE
        ROOM_PRICE AS actual_nightly_rate,
        
        -- 每 10 块钱为一个价格段
        floor(ROOM_PRICE / 10) * 10 AS price_bucket_start,
        floor(ROOM_PRICE / 10) * 10 + 10 AS price_bucket_end
    FROM 
        hotel.dwd_hotel_order 
    WHERE 
        OUT_DATE >= '2023-01-01' 
        AND OUT_DATE < '2026-01-01' 
        AND STATUS = '已离店' 
        AND (ROOM_PRICE > 0 OR ROOM_NIGHTS > 0)
        AND SUPPLIER_NO IN ('天下房仓1', '差旅壹号1')
        AND (EP_TYPE != '测试' OR EP_TYPE IS NULL)
),
BucketAgg AS (
    -- 按价格段、订单类型和年份聚合数据
    SELECT 
        ORDER_TYPE,
        price_bucket_start,
        price_bucket_end,
        
        -- 2023 年统计
        SUM(CASE WHEN order_year = 2023 THEN real_room_nights ELSE 0 END) AS nights_23,
        SUM(CASE WHEN order_year = 2023 THEN real_room_nights * actual_nightly_rate ELSE 0 END) / 
            NULLIF(SUM(CASE WHEN order_year = 2023 THEN real_room_nights ELSE 0 END), 0) AS avg_price_23,
            
        -- 2024 年统计
        SUM(CASE WHEN order_year = 2024 THEN real_room_nights ELSE 0 END) AS nights_24,
        SUM(CASE WHEN order_year = 2024 THEN real_room_nights * actual_nightly_rate ELSE 0 END) / 
            NULLIF(SUM(CASE WHEN order_year = 2024 THEN real_room_nights ELSE 0 END), 0) AS avg_price_24,
            
        -- 2025 年统计
        SUM(CASE WHEN order_year = 2025 THEN real_room_nights ELSE 0 END) AS nights_25,
        SUM(CASE WHEN order_year = 2025 THEN real_room_nights * actual_nightly_rate ELSE 0 END) / 
            NULLIF(SUM(CASE WHEN order_year = 2025 THEN real_room_nights ELSE 0 END), 0) AS avg_price_25
    FROM 
        OrderData
    GROUP BY 
        ORDER_TYPE,
        price_bucket_start,
        price_bucket_end
)
SELECT 
    ORDER_TYPE AS "因公/因私",
    concat(toString(price_bucket_start), ' - ', toString(price_bucket_end)) AS "价格段",
    
    -- 2023 结果
    nights_23 AS "2023年_间夜量",
    avg_price_23 AS "2023年_该段平均间夜单价",
    
    -- 2024 结果
    nights_24 AS "2024年_间夜量",
    avg_price_24 AS "2024年_该段平均间夜单价",
    
    -- 2025 结果
    nights_25 AS "2025年_间夜量",
    avg_price_25 AS "2025年_该段平均间夜单价"
    
FROM 
    BucketAgg
ORDER BY 
    ORDER_TYPE,
    price_bucket_start;