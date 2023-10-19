# To run the tests

In a seperate terminal (run local solana validator) - 

```
solana-test-validator --config config.yml --reset
```

In a seperate terminal (run the test) - 

```
anchor test --skip-local-validator
```