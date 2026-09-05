import time
from omnibook import Client

env_dict = {}
with open(".env") as f:
    for line in f:
        if "=" in line:
            k, v = line.strip().split("=", 1)
            env_dict[k] = v.strip()

c = Client(api_key=env_dict["OMNIBOOK_API_KEY"], api_secret=env_dict["OMNIBOOK_API_SECRET"])

markets = c.get_markets()
trading = next((m for m in markets.get("markets", []) if m["status"] == "trading"), None)
if not trading and markets.get("markets"):
    trading = markets["markets"][0]
mid = trading["market_id"] if trading else 64
print(f"Trading market: {mid}")

cid = str(int(time.time() * 1000))

print("\n--- Test A: type='limit' (plain) ---")
try:
    r = c.place_order(client_order_id=cid+"1", market_id=mid, side="buy", outcome="yes", qty=1, tick=5000, type="limit")
    print(f"SUCCESS: {r}")
except Exception as e:
    print(f"FAILED: {e}")

print("\n--- Test B: side='BUY', outcome='YES' ---")
try:
    r = c.place_order(client_order_id=cid+"2", market_id=mid, side="BUY", outcome="YES", qty=1, tick=5000, type="limit")
    print(f"SUCCESS: {r}")
except Exception as e:
    print(f"FAILED: {e}")

print("\n--- Test C: side='buy', outcome='yes', tif='gtc' ---")
try:
    r = c.place_order(client_order_id=cid+"3", market_id=mid, side="buy", outcome="yes", qty=1, tick=5000, type="limit", tif="gtc")
    print(f"SUCCESS: {r}")
except Exception as e:
    print(f"FAILED: {e}")

print("\n--- Test D: side='buy', outcome='yes', tif='ioc' ---")
try:
    r = c.place_order(client_order_id=cid+"4", market_id=mid, side="buy", outcome="yes", qty=1, tick=5000, type="limit", tif="ioc")
    print(f"SUCCESS: {r}")
except Exception as e:
    print(f"FAILED: {e}")

