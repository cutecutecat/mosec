# Multiply routes

Mosec supports multiple routing inference, allowing users to specify corresponding route for each `Worker` and build multiple pipelines on the same server to perform related but hard-to-merged inferences.

This feature is particularly useful where the request structure of different inference processes varies greatly. From a software engineering perspective, forcing merge these inferences will break Python type hints and increase code coupling.

Each route has the full feature of `Server` and `Worker`. The code style of whether to use routing is kept as consistent as possible. When the user does not specify a route, the built worker will fallback to the default  `/inference` route to ensure forward compatibility.

This is an example demonstrating how to use multiply routes.

## Server

```shell
python multi_routes.py
```

<details>
<summary>multi_routes.py</summary>

```{include}
:code: python
```

</details>

## Start

```shell
python echo.py
```

## Test

```shell
http :8000/math/next num=9
http :8000/math/even num=9
http :8000/ping num=9
```
