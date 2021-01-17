# Rust-ML

A simple but usable complete machine learning library with written in Rust.

## Contents

- **Neuron**: A simple network library.
- **RL**: Implementations of RL algorithms and training environments.

## Neuron

CPU based neural network library implemented using [ndarray](https://github.com/rust-ndarray/ndarray).
It won't outperform Tensorflow but it should still be very fast.

## RL

Reinforcement learning library containing Agents, Learners and Environments. 


An **agent** interacts with and **environment** and learns using a **learner**.
All agents, learners and environments are designed to be easily swappable. For
example a QAgent can interact with a Jump environment and learn using a QLearner,
and that same agent can interact with a Bird environment and learn using a
NeuroEvolutionLearner.