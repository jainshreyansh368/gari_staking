use kafka::producer::{Producer, Record, RequiredAcks};
use std::time::Duration;
use tracing::info;

pub struct KafkaProducer {
    producer: Producer,
}

impl KafkaProducer {
    pub fn init(hosts: Vec<String>, timeout: u64) -> Result<Self, String> {
        let kafka_producer: Producer = match Producer::from_hosts(hosts.clone())
            .with_ack_timeout(Duration::from_millis(timeout))
            .with_required_acks(RequiredAcks::One)
            .create()
        {
            Ok(p) => {
                info!("Initialized producer for hosts {:?}", hosts.len());
                p
            }
            Err(err) => return Err(std::format!("Error in initialize {:?}", err.to_string())),
        };

        Ok(Self {
            producer: kafka_producer,
        })
    }

    pub fn send_message_to_topic(&mut self, data: String, topic: &str) -> String {
        let record = Record::from_value(&topic, data.as_bytes());
        match self.producer.send(&record) {
            Ok(_) => {
                //self.producer.flush(Duration::from_secs(10));
                return format!("Sent message to topic. Not waiting for confirmation!");
            }
            Err(e) => return format!("Error in sending message {:?}", e.to_string()),
        }
    }
}
