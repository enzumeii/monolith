"""Serve published local report files; configuration is owned by the API TOML."""
import argparse
import functools
import http.server
import ipaddress
import pathlib
import tomllib
import urllib.parse


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("configuration", type=pathlib.Path)
    args = parser.parse_args()
    with args.configuration.open("rb") as source:
        storage = tomllib.load(source)["reports"]["storage"]
    if storage["backend"] != "local":
        parser.error("serve-reports requires the local storage backend")
    url = urllib.parse.urlsplit(storage["public_base_url"])
    host = url.hostname
    if url.scheme != "http" or not host or (host != "localhost" and not ipaddress.ip_address(host).is_loopback):
        parser.error("local serving requires a loopback HTTP public_base_url")
    if url.query or url.fragment or url.username or url.password:
        parser.error("invalid local storage URL")
    directory = pathlib.Path(storage["directory"])
    if not directory.is_absolute():
        directory = args.configuration.resolve().parent / directory
    directory = directory / "public"
    directory.mkdir(parents=True, exist_ok=True)
    prefix = url.path.rstrip("/") + "/"

    class PublishedFiles(http.server.SimpleHTTPRequestHandler):
        def do_GET(self):
            path = urllib.parse.urlsplit(self.path).path
            if not path.startswith(prefix) or not path.endswith(".csv"):
                self.send_error(404)
                return
            self.path = "/" + path[len(prefix):]
            super().do_GET()

        def do_HEAD(self):
            self.send_error(405)

        def end_headers(self):
            if self.path.endswith(".csv"):
                self.send_header("Content-Disposition", "attachment")
            super().end_headers()

    handler = functools.partial(PublishedFiles, directory=str(directory))
    with http.server.ThreadingHTTPServer(("127.0.0.1", url.port or 80), handler) as server:
        print(storage["public_base_url"], flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
