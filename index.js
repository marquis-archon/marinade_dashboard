/*
 * Primary file for API
 *
 */

// Dependencies
var http = require("http");
var https = require("https");
var url = require("url");
var StringDecoder = require("string_decoder").StringDecoder;
var config = require("./lib/config");
var fs = require("fs");
var handlers = require("./lib/handlers");
var helpers = require("./lib/helpers");
var shell = require('shelljs');

var CronJob = require('cron').CronJob;
var job = new CronJob('00 00 11 * * *', function() {
  const d = new Date();
  console.log('11:00 AM: job run:', d);
  // Update db 
  shell.exec('cd stake-o-matic-master && cargo build && export VALIDATORS_APP_TOKEN=5TFNgpCnuRZz6mkk3oyNFbin && bash clean-score-all-mainnet.sh',
    // Delete current validators json
    fs.unlink('./.data/epochs/validators.json', (err) => {
    if (err) throw err;
    console.log('File deleted');
    /* Generate new validators json
     Call stake o matic to update db */
     helpers.generateValidators()
  }));
});
job.start();

// shell.exec('cd stake-o-matic-master && cargo build && export VALIDATORS_APP_TOKEN=5TFNgpCnuRZz6mkk3oyNFbin && bash clean-score-all-mainnet.sh',helpers.generateValidators());

const PORT = process.env.PORT || 8080;
// Instantiate the HTTP server
var httpServer = http.createServer(function (req, res) {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'OPTIONS, GET');
  res.setHeader('Access-Control-Max-Age', 2592000)
  unifiedServer(req, res);
});

// Start the HTTP server
httpServer.listen(PORT,"0.0.0.0",function () {
  console.log("The HTTP server is running on port " + 8081);
});

// Instantiate the HTTPS server
var httpsServerOptions = {
  key: fs.readFileSync("./https/key.pem"),
  cert: fs.readFileSync("./https/cert.pem"),
};
var httpsServer = https.createServer(httpsServerOptions, function (req, res) {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'OPTIONS, GET');
  res.setHeader('Access-Control-Max-Age', 2592000)
  unifiedServer(req, res);
});
// Start the HTTPS server
httpsServer.listen(8081, function () {
  console.log("The HTTPS server is running on port " + 8080);
});

// All the server logic for both the http and https server
var unifiedServer = function (req, res) {

  console.log(req.connection.localAddress, req.connection.localPort);
  // Parse the url
  var parsedUrl = url.parse(req.url, true);

  // Get the path
  var path = parsedUrl.pathname;
  var trimmedPath = path.replace(/^\/+|\/+$/g, "");
  var nestedRoute = trimmedPath.split("/");

  // Get the query string as an object
  var queryStringObject = parsedUrl.query;

  var page = queryStringObject.page;
  var limit = queryStringObject.limit;

  // Get the HTTP method
  var method = req.method.toLowerCase();

  //Get the headers as an object
  var headers = req.headers;

  // Get the payload,if any
  var decoder = new StringDecoder("utf-8");
  var buffer = "";
  req.on("data", function (data) {
    buffer += decoder.write(data);
  });
  req.on("end", function () {
    buffer += decoder.end();
    // console.log(nestedRoute);
    // Check the router for a matching path for a handler. If one is not found, use the notFound handler instead.
    var chosenHandler =
      typeof router[nestedRoute[0]] !== "undefined"
        ? router[nestedRoute[0]]
        : handlers.notFound;

    // console.log(trimmedPath);

    // Construct the data object to send to the handler
    var data = {
      trimmedPath: trimmedPath,
      nestedRoute: nestedRoute,
      queryStringObject: queryStringObject,
      method: method,
      headers: headers,
      payload: helpers.parseJsonToObject(buffer),
      page: page,
      limit: limit,
    };
    // console.log(data);
    // Route the request to the handler specified in the router
    chosenHandler(data, function (statusCode, payload) {
      // Use the status code returned from the handler, or set the default status code to 200
      statusCode = typeof statusCode == "number" ? statusCode : 200;

      // Use the payload returned from the handler, or set the default payload to an empty object
      payload = typeof payload == "object" ? payload : {};

      // Convert the payload to a string
      var payloadString = JSON.stringify(payload);

      // Return the response
      res.setHeader("Content-Type", "application/json");
      res.writeHead(statusCode);
      res.end(payloadString);
      // console.log(trimmedPath, statusCode);
    });
  });
};

// Define the request router
var router = {
  ping: handlers.ping,
  users: handlers.users,
  tokens: handlers.tokens,
  validators: handlers.validators,
};
