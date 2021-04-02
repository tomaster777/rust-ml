use ndarray::prelude::*;

use crate::neuron::layers::Cached;
use crate::neuron::losses::Loss;
use crate::neuron::networks::CachedRegression;
use crate::neuron::optimizers::{OptimizeBatch, OptimizeOnce};

#[derive(Clone)]
pub struct SGD {
    loss: Loss,
}

impl SGD {
    pub fn new(loss: Loss) -> Self {
        Self { loss }
    }

    fn chain_rule_weights(
        &self,
        dl_da: &Array1<f32>,
        da_dt: &Array1<f32>,
        dt_dw: &Array1<f32>,
    ) -> Array2<f32> {
        let layer_outputs = da_dt.len();
        let layer_inputs = dt_dw.len();

        // gradient of the loss with respect to the transfer of the current layer
        // transposed for matrix multiplication
        let dl_dt = Array2::from_shape_vec((layer_outputs, 1), (dl_da * da_dt).to_vec()).unwrap();

        // convert gradient of the transitions with respect to the weigts into a 2d
        // array with one row
        let dt_dw = Array2::from_shape_vec((1, layer_inputs), dt_dw.to_vec()).unwrap();

        // matrix with dimensions layer_outputs X layer_inputs
        let layer_weights_gradients = dl_dt.dot(&dt_dw);

        layer_weights_gradients
    }

    fn chain_rule_biases(&self, dl_da: &Array1<f32>, da_dt: &Array1<f32>) -> Array1<f32> {
        // derivatives of the transfers with respect to the biases (dt_db) is 1
        // so da_db = da_dt * dt_db = da_dt * 1 = da_dt
        let da_db = da_dt;

        // chain rule - derivatives of the loss with respect to the biases
        let layer_biases_gradient = dl_da * da_db;

        layer_biases_gradient
    }

    fn chain_rule_previous_activations(
        &self,
        dl_da: &Array1<f32>,
        da_dt: &Array1<f32>,
        dt_dap: &Array2<f32>,
    ) -> Array1<f32> {
        // since each node in the previous can affect multiple nodes in this layer
        // the derivatives are summed over all nodes affected in this layer (Axis(0))
        let dl_da_sum = dl_da.sum();
        let da_dt_sum = da_dt.sum();
        let dt_dap_sum = dt_dap.sum_axis(Axis(0));

        // chain rule - derivatives of loss with respect to previous layer's
        // activations
        let previous_layer_activations_gradient = dl_da_sum * da_dt_sum * dt_dap_sum;

        previous_layer_activations_gradient
    }

    fn get_gradients<N, L>(
        &self,
        network: &mut N,
        input: &Array1<f32>,
        expected: &Array1<f32>,
    ) -> (Vec<Array2<f32>>, Vec<Array1<f32>>)
    where
        L: Cached,
        N: CachedRegression<L>,
    {
        let mut network_weights_gradients = vec![];
        let mut network_biases_gradients = vec![];

        let prediction = network.predict_cached(input);

        // derivatives of the loss with respect to the last layers activation
        let mut dl_da = Box::new(self.loss.derivative(&prediction, expected));

        for layer in network.get_layers().iter().rev() {
            // derivatives of the activations with respect to the transfers
            // NOTE: unwrap is safe since we called `predict_cached`
            let da_dt = layer.apply_activation_derivative(layer.get_transfer().unwrap());

            // derivatives of the transfers with respect to the weights - these are
            // the activations of the previous layer, which is also the input to the
            // current layer
            // NOTE: unwrap is safe since we called `predict_cached`
            let dt_dw = layer.get_input().unwrap();

            // derivatives of the transfers with respect to the previous layer's
            // activations - these are all the weights from each node in the
            // previous layer
            let dt_dap = layer.get_weights();

            // derivatives of the losses with respect to the biases
            let dl_db = self.chain_rule_biases(&dl_da, &da_dt);
            network_biases_gradients.insert(0, dl_db);

            // derivatives of the losses with respect to the weights
            let dl_dw = self.chain_rule_weights(&dl_da, &da_dt, &dt_dw);
            network_weights_gradients.insert(0, dl_dw);

            // derivatives of the losses with respect to the previous layers activations
            let dl_dap = self.chain_rule_previous_activations(&dl_da, &da_dt, &dt_dap);

            // BACK PROPAGATION: set the loss with respect to the current layer's
            // activations as the the loss with respect to the *previous* layer's
            // activations, propagating the loss to the previous layers
            dl_da = Box::new(dl_dap);
        }

        (network_weights_gradients, network_biases_gradients)
    }

    fn get_batch_gradients<N, L>(
        &self,
        network: &mut N,
        batch_inputs: &[Array1<f32>],
        batch_expected: &[Array1<f32>],
    ) -> (Vec<Array2<f32>>, Vec<Array1<f32>>)
    where
        L: Cached,
        N: CachedRegression<L>,
    {
        assert_eq!(
            batch_inputs.len(),
            batch_expected.len(),
            "batch inputs and expected must be of same length"
        );

        // gradients are empty if batch is empty
        if batch_inputs.is_empty() {
            return (vec![], vec![]);
        }

        // calculate batch gradients
        let batch_length = batch_inputs.len() as f32;
        batch_inputs
            .iter()
            .zip(batch_expected.iter())
            .map(|(input, expected)| {
                let (weight_gradients, bias_gradients) =
                    self.get_gradients(network, input, expected);

                (
                    weight_gradients
                        .iter()
                        .map(|layer_weights_gradients| layer_weights_gradients / batch_length)
                        .collect(),
                    bias_gradients
                        .iter()
                        .map(|layer_biases_gradients| layer_biases_gradients / batch_length)
                        .collect(),
                )
            })
            .reduce(
                |(total_weights_gradients, total_biases_gradients): (
                    Vec<Array2<f32>>,
                    Vec<Array1<f32>>,
                ),
                 (weights_gradients, biases_gradients)| {
                    (
                        total_weights_gradients
                            .iter()
                            .zip(weights_gradients.iter())
                            .map(|(total_layer_weights_gradients, layer_weights_gradients)| {
                                total_layer_weights_gradients + layer_weights_gradients
                            })
                            .collect(),
                        total_biases_gradients
                            .iter()
                            .zip(biases_gradients.iter())
                            .map(|(total_layer_biases_gradients, layer_biases_gradients)| {
                                total_layer_biases_gradients + layer_biases_gradients
                            })
                            .collect(),
                    )
                },
            )
            .unwrap()
    }
}

impl<N, L> OptimizeOnce<N, L> for SGD
where
    L: Cached,
    N: CachedRegression<L>,
{
    fn optimize_once(
        &self,
        network: &mut N,
        input: &Array1<f32>,
        expected: &Array1<f32>,
        learning_rate: f32,
    ) {
        let (weight_gradients, bias_gradients) = self.get_gradients(network, input, expected);

        for (weights, gradients) in network.get_weights_mut().iter_mut().zip(weight_gradients) {
            **weights = weights.clone() - gradients * learning_rate;
        }

        for (biases, gradients) in network.get_biases_mut().iter_mut().zip(bias_gradients) {
            **biases = biases.clone() - gradients * learning_rate;
        }
    }
}

impl<N, L> OptimizeBatch<N, L> for SGD
where
    L: Cached,
    N: CachedRegression<L>,
{
    fn optimize_batch(
        &self,
        network: &mut N,
        batch_inputs: &[Array1<f32>],
        batch_expected: &[Array1<f32>],
        learning_rate: f32,
    ) {
        let (weights_gradients, biases_gradients) =
            self.get_batch_gradients(network, batch_inputs, batch_expected);

        for (weights, gradients) in network.get_weights_mut().iter_mut().zip(weights_gradients) {
            **weights = weights.clone() - gradients * learning_rate;
        }

        for (biases, gradients) in network.get_biases_mut().iter_mut().zip(biases_gradients) {
            **biases = biases.clone() - gradients * learning_rate;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::neuron::activations::{LeakyReLu, ReLu, Sigmoid, Softplus};
    use crate::neuron::layers::CachedLayer;
    use crate::neuron::losses::{mse_loss, sse_loss, MSE, SSE};
    use crate::neuron::networks::{CachedNetwork, Regression};
    use crate::neuron::transfers::Dense;

    use super::*;

    #[test]
    fn test_sgd_optimize_batch_sin_convergence() {
        let mut network = CachedNetwork::new(vec![
            CachedLayer::new(3, 1, Dense::new(), Sigmoid::new()),
            CachedLayer::new(1, 3, Dense::new(), Sigmoid::new()),
        ]);

        let batch_inputs: Vec<Array1<f32>> = Array1::linspace(0.1, 0.9, 100)
            .iter()
            .map(|&x| array![x])
            .collect();

        let batch_expected: Vec<Array1<f32>> = Array1::linspace(0.1, 0.9, 100)
            .iter()
            .map(|&x| array![(x as f32).sin()])
            .collect();

        let optimizer = SGD::new(SSE::new());

        for e in 0..1_000 {
            let mut cost = 0.;
            for (input, expected) in batch_inputs.iter().zip(batch_expected.iter()) {
                let prediction = network.predict(&input);
                cost += sse_loss(&prediction, &expected).sum();
            }

            if e & 100 == 0 {
                eprintln!("epoch: {} cost: {}", e, cost / 100.);
            }

            optimizer.optimize_batch(&mut network, &batch_inputs, &batch_expected, 5.);
        }

        let mut total_cost = 0.;
        for (input, expected) in batch_inputs.iter().zip(batch_expected.iter()) {
            let prediction = network.predict(&input);
            let cost = sse_loss(&prediction, &expected).sum();
            eprintln!(
                "prediction: {} expected: {}",
                prediction.to_string(),
                expected.to_string()
            );
            total_cost += cost / 100.;
        }

        assert!(
            total_cost <= 0.001,
            "optimizer failed to converge (cost: {}>0.001)",
            total_cost
        );
    }

    #[test]
    fn test_sgd_optimize_once_convergence() {
        let mut network = CachedNetwork::new(vec![
            CachedLayer::new(3, 2, Dense::new(), Softplus::new()),
            CachedLayer::new(4, 3, Dense::new(), ReLu::new()),
            CachedLayer::new(5, 4, Dense::new(), Sigmoid::new()),
            CachedLayer::new(6, 5, Dense::new(), LeakyReLu::new()),
        ]);

        let input = array![1., 0.];
        let expected = array![0.0, 0.2, 0.4, 0.6, 0.8, 1.0];

        let optimizer = SGD::new(MSE::new());

        for _ in 0..200 {
            optimizer.optimize_once(&mut network, &input, &expected, 0.1);
        }

        let prediction = network.predict(&input);
        let cost = mse_loss(&prediction, &expected).sum();
        eprintln!(
            "prediction: {} expected: {}",
            prediction.to_string(),
            expected.to_string()
        );
        assert!(
            cost <= 0.0001,
            "optimizer failed to converge (cost: {}>0.0001)",
            cost
        );
    }
}
