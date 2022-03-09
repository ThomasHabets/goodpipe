# Goodpipe

Goodpipe is a tool to create better data pipelines. Its scope is restricted to
things that run on the local machine.

This is not an official Google product.

## What problem it's solving

When you run `foo | bar`, there's no way for `bar` to know if `foo` actually
succeeded. If `bar` "commits" the data once it's complete, then when `foo` fails
`bar` will have committed bad data.

Examples of "committing":

* `bar` is uploading to a database, and runs `COMMIT` when the input ends. We
  don't want partial data to be committed.
* `bar` uploads to some cloud storage, where it has to "close" a file upload. We
  don't want partial (or empty!) files to be uploaded.

## Building

```
$ go get github.com/ThomasHabets/goodpipe
```

## Example

```shell
$ cat > test.pipe.json
[
  ["sort", "-S300M", "input.txt"],
  ["gsutil", "cp", "-", "gs://example/input-sorted.txt"]
]
$ ./goodpipe < test.pipe.json

$ goodpipe <<EOF
[
  ["gsutil", "cat", "gs://example/input-unsorted.txt"],
  ["sort", "-S300M", "-n", "input.txt"],
  ["gzip", "-9"],
  ["gsutil", "cp", "-", "gs://example/input-sorted-numerically.txt.gz"]
]
EOF
```
