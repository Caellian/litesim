## connectors

Connectors lie on the edges of models and are used to connect them to other models.

They're defined by their IDs and types. Input connectors additionaly have to specify a handler.

## Input connector handler

Handler functions on input connectors get called on recieving an event and mutate the model as needed based on received input.

Their return type is `Result<(), SimulationError>`