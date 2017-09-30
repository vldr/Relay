var fs = require('fs');
var WebSocketServer = require('ws').Server;

process.on('uncaughtException', function (err) {
  console.error(err);
  console.log("Node NOT Exiting...");
});

var wss = new WebSocketServer({port: 1234, host:"vldr.org"});
var rooms = {};
var roomsFileInfo = {};
wss.on('connection', function(ws) {  
	ws.on('message', function(msg)
	{
		if (typeof msg !== 'string') {
			var memberOf = "";
			
			for (var key in rooms) 
			{
				if (rooms.hasOwnProperty(key)) 
				{
					var clients = rooms[key];
					
					if (clients.length > 0) {
						if (clients[0] == ws) {
							memberOf = key;
						}
					}
				}
			}
			
			if (rooms[memberOf]) 
			{
				var clients = rooms[memberOf];
				
				if (clients.length == 1)
				{
					console.log("Closing connection because no client is inside the server...");
					clients[0].close();
					clients.splice(0, 1);
					delete roomsFileInfo[memberOf];
					delete rooms[memberOf];
					
					return;
				}
				
				clients[1].send(msg, { binary: true });
			}		
		} else {
			//console.log(msg);
			var message = JSON.parse(msg);
			switch(message.status) {
				// Join room.
				case 1:
					if (rooms[message.msg]) {
						// Send file details...
						var clients = rooms[message.msg];
						
						if (clients.length == 1) {
							console.log("Client has entered room " + message.msg);
							ws.send(JSON.stringify({status: 0, msg: roomsFileInfo[message.msg]}));
							rooms[message.msg].push(ws);
							
							clients[0].send("start");
						} else {
							console.log("You can't enter room " + message.msg + " because it's full!");
							ws.send(JSON.stringify({status: 1, msg: "You can't enter room " + message.msg + " because it's full!"}));
							ws.close();
						}
					} else {
						console.log("You can't enter room " + message.msg + " because it doesn't exist...");
						ws.send(JSON.stringify({status: 1, msg: "You can't enter room " + message.msg + " because it doesn't exist..."}));
						ws.close();
					}
					
					break;
				// Create room...
				case 0:
					var hash = "";
					var chars = "abcdefghijklmnopqrstuvwxyz0123456789";
					for (var i = 0; i < 4; i++)
						hash += chars.charAt(Math.floor(Math.random() * chars.length));

					
					if (!rooms[hash]) {
						rooms[hash] = [];
						rooms[hash].push(ws);
						
						roomsFileInfo[hash] = JSON.stringify({ filesize: message.filesize, filetype: message.filetype, filename: message.filename });
						console.log("Creating room " + hash + "...");
						
						ws.send(JSON.stringify({ status: 0, msg: hash }));
					} else {
						console.log("Error creating room...");
						ws.send(JSON.stringify({ status: 1, msg: "Error creating room..." }));
					}	
					break;
				default:
					console.log('Unknown message [Status Code: ' + message.status + ' Msg: ' + message.msg + ']');
					break;
			}
		}
	});
  
	ws.on('close', function() {
		for (var key in rooms) {
			if (rooms.hasOwnProperty(key)) {
				var clients = rooms[key];
				
				for (var i = 0; i < clients.length; i++) {
					if (clients[i] == ws) {
						console.log("Deleting client " + i + " from room " + key + "...");
						clients.splice(i, 1);
					}
				}
				
				if (clients.length == 0) {
					console.log("Deleting room " + key + " because it's empty...");
					delete roomsFileInfo[key];
					delete rooms[key];
				}
			}
		}
		
		console.log('Client has disconnected...');
	});
});
