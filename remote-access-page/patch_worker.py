#! /usr/bin/python

with open("patch.js") as patch_f:
    patch = patch_f.read()

with open("dist/worker.js", "r+") as worker_f:
    worker = worker_f.read()

worker = worker.replace("new Client", "client = new Client")
worker = worker + patch
with open("dist/worker.js", "w") as worker_f:
    worker_f.write(worker)
