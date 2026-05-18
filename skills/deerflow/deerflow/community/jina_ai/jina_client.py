import os

import httpx


class JinaClient:
    async def crawl(self, url: str, return_format: str = "html", timeout: int = 10) -> str:
        headers = {
            "Content-Type": "application/json",
            "X-Return-Format": return_format,
            "X-Timeout": str(timeout),
        }
        if os.getenv("JINA_API_KEY"):
            headers["Authorization"] = f"Bearer {os.getenv('JINA_API_KEY')}"
        data = {"url": url}
        try:
            async with httpx.AsyncClient() as client:
                response = await client.post("https://r.jina.ai/", headers=headers, json=data, timeout=timeout)

            if response.status_code != 200:
                return "Error: 网页获取失败，请稍后重试"

            content = response.content.decode("utf-8", errors="replace")
            if not content or not content.strip():
                return "Error: 网页内容为空"

            return content
        except httpx.TimeoutException:
            return "Error: 网页获取超时，请稍后重试"
        except Exception:
            return "Error: 网页获取失败，请稍后重试"

            content = response.content.decode("utf-8", errors="replace")
            if not content or not content.strip():
                logger.error("Jina API returned empty response")
                return "Error: 网页内容为空"

            return content
        except httpx.TimeoutException:
            logger.warning("Jina API timeout for url: %s", url)
            return "Error: 网页获取超时，请稍后重试"
        except Exception:
            logger.warning("Jina API request failed")
            return "Error: 网页获取失败，请稍后重试"
