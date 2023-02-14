# Metrics Aggregator Example

Simulates Nym-Api metrics collection and rewarding. The goal is to try out how to integrate Ephemera with actual
Nym-Api.

## Current Nym-Api(simulated)

* Runs a **metrics collector** which simulates metrics collection from mixnodes.
    * Generates random metrics for each mixnode and saves it in database.
* Runs a **reward distributor** which simulates reward distribution.
    * Aggregates the metrics collected by the **metrics collector** from the database.
    * Sends the aggregated metrics to the **"smart contract"**.
* Runs a **"smart contract"**(just a http server) which listens for reward distribution requests.
    * It just stores the aggregated metrics in a database for introspection.

## **TODO** Nym-Api with Ephemera(simulated)

* Runs a **metrics collector** which simulates metrics collection from mixnodes.
    * Generates random metrics for each mixnode and saves them in a database.
* Runs a **reward distributor** which simulates reward distribution.
    * Aggregates the metrics collected by the **metrics collector** from the database.
    * **--------------------------------DIFFERENCE----------------------------------------------**
    * **Uses Ephemera to distribute the local aggregated metrics to other Ephemera nodes**
    * **After the local Ephemera node have finalized local block with the aggregated metrics from all nodes,
      calculates the summary of the aggregated metrics and tries to send it to the "smart contract".**
    * **"Smart contract" accepts only the first request(all nodes race to submit it)**
    * **--------------------------------END OF DIFFERENCE--------------------------------**
* Runs a **"smart contract"**(just a http server) which listens for reward distribution requests.
    * It just stores the aggregated metrics in a database for introspection.

## How to run

```bash
RUST_LOG="info" cargo run
```