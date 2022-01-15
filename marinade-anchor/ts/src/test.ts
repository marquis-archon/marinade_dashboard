#!/usr/bin/env ts-node

const { run } = require(".");
run().then(
    () => process.exit(),
    (err: any) => {
        console.error(err);
        process.exit(-1);
    }
);