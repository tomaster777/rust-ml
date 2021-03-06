use ndarray::Array1;

use crate::neuron::activations::{sigmoid_activation, Activation};

pub fn softplus_activation(transfer: &Array1<f32>) -> Array1<f32> {
    transfer.map(|&x| (1. + f32::exp(x)).ln())
}

pub fn softplus_derivative(transfer: &Array1<f32>) -> Array1<f32> {
    sigmoid_activation(transfer)
}

pub fn softplus() -> Activation {
    Activation::new(softplus_activation, softplus_derivative)
}
